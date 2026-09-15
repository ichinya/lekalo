//! Windows LPAC process launch. All ACL grants target fresh private staging;
//! no project, executable installation, or user-wide ACL is changed.
//! Unsafe calls are confined here, with owned handles/attributes/profile
//! lifetimes spanning CreateProcess and a kill-on-close job before resume.

use super::{
    transport::{self, Process, TransportFailure as Failure},
    Sandbox,
};
use std::{
    ffi::c_void,
    fs::File,
    io::{Read, Write},
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    ptr::{null, null_mut},
    sync::atomic::{AtomicBool, Ordering},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, Isolation::*, *},
    Storage::FileSystem::*,
    System::{JobObjects::*, Pipes::*, Threading::*},
};

fn wide(value: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}
#[track_caller]
fn io_error() -> Failure {
    #[cfg(test)]
    eprintln!(
        "Windows sandbox API error at {}: {:?}",
        std::panic::Location::caller(),
        std::io::Error::last_os_error().raw_os_error()
    );
    Failure::Spawn
}
fn handle(raw: HANDLE) -> Result<OwnedHandle, Failure> {
    if raw.is_null() || raw == INVALID_HANDLE_VALUE {
        Err(io_error())
    } else {
        Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
    }
}

struct Profile {
    name: Vec<u16>,
    sid: PSID,
    deleted: bool,
}

// An LPAC can read explicitly shared package objects elsewhere on the host.
// A project with such grants is not a confineable private project: reject
// it before any adapter code, rather than editing the user's ACLs.
pub(super) fn check_project_boundary(root: &Path) -> Result<(), Failure> {
    use std::os::windows::fs::MetadataExt;
    let capabilities = Capabilities::registry_read()?;
    let mut shared = null_mut();
    if unsafe { ConvertStringSidToSidW(wide("S-1-15-2-2").as_ptr(), &mut shared) } == 0 {
        return Err(io_error());
    }
    struct Sid(PSID);
    impl Drop for Sid {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
    let shared = Sid(shared);
    let mut pending = vec![root.to_path_buf()];
    let mut count = 0;
    while let Some(path) = pending.pop() {
        count += 1;
        if count > 100_000 {
            return Err(Failure::Spawn);
        }
        let metadata = std::fs::symlink_metadata(&path).map_err(|_| Failure::Spawn)?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(Failure::Spawn);
        }
        let mut acl = null_mut();
        let mut descriptor = null_mut();
        let code = unsafe {
            GetNamedSecurityInfoW(
                wide(&path).as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                &mut acl,
                null_mut(),
                &mut descriptor,
            )
        };
        if code != ERROR_SUCCESS {
            return Err(Failure::Spawn);
        }
        struct Descriptor(PSECURITY_DESCRIPTOR);
        impl Drop for Descriptor {
            fn drop(&mut self) {
                unsafe {
                    LocalFree(self.0);
                }
            }
        }
        let _descriptor = Descriptor(descriptor);
        if acl.is_null() {
            return Err(Failure::Spawn);
        }
        for index in 0..unsafe { (*acl).AceCount } {
            let mut ace = null_mut();
            if unsafe { GetAce(acl, index as u32, &mut ace) } == 0 {
                return Err(Failure::Spawn);
            }
            let header = unsafe { &*ace.cast::<ACE_HEADER>() };
            // Standard allow/deny ACEs are supported; object/callback ACLs
            // need their own proven evaluator and therefore fail closed.
            if header.AceType == 1 {
                continue;
            }
            if header.AceType != 0 {
                return Err(Failure::Spawn);
            }
            let entry = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
            let sid = std::ptr::addr_of!(entry.SidStart).cast_mut().cast();
            if unsafe { EqualSid(sid, shared.0) } != 0
                || (0..capabilities.count)
                    .any(|i| unsafe { EqualSid(sid, *capabilities.sids.add(i as usize)) != 0 })
            {
                return Err(Failure::Spawn);
            }
        }
        if metadata.is_dir() {
            for child in std::fs::read_dir(path).map_err(|_| Failure::Spawn)? {
                if pending.len() + count >= 100_000 {
                    return Err(Failure::Spawn);
                }
                pending.push(child.map_err(|_| Failure::Spawn)?.path());
            }
        } else if !metadata.is_file() {
            return Err(Failure::Spawn);
        }
    }
    Ok(())
}

struct Capabilities {
    groups: *mut PSID,
    group_count: u32,
    sids: *mut PSID,
    count: u32,
}
impl Capabilities {
    fn registry_read() -> Result<Self, Failure> {
        let mut result = Self {
            groups: null_mut(),
            group_count: 0,
            sids: null_mut(),
            count: 0,
        };
        if unsafe {
            DeriveCapabilitySidsFromName(
                wide("registryRead").as_ptr(),
                &mut result.groups,
                &mut result.group_count,
                &mut result.sids,
                &mut result.count,
            )
        } == 0
        {
            return Err(io_error());
        }
        Ok(result)
    }
}
impl Drop for Capabilities {
    fn drop(&mut self) {
        unsafe {
            for index in 0..self.group_count {
                LocalFree(*self.groups.add(index as usize));
            }
            for index in 0..self.count {
                LocalFree(*self.sids.add(index as usize));
            }
            LocalFree(self.groups.cast());
            LocalFree(self.sids.cast());
        }
    }
}
impl Profile {
    fn new(sandbox: &Sandbox) -> Result<Self, Failure> {
        let name = wide(format!(
            "lekalo.{}",
            sandbox
                .owned
                .path()
                .file_name()
                .ok_or(Failure::Spawn)?
                .to_string_lossy()
        ));
        let mut sid = null_mut();
        // The random private directory name is also the profile custody
        // identity. Existing profiles are never adopted or deleted.
        let result = unsafe {
            CreateAppContainerProfile(
                name.as_ptr(),
                name.as_ptr(),
                name.as_ptr(),
                null(),
                0,
                &mut sid,
            )
        };
        if result < 0 {
            #[cfg(test)]
            eprintln!("CreateAppContainerProfile HRESULT: {result:#x}");
            return Err(io_error());
        }
        Ok(Self {
            name,
            sid,
            deleted: false,
        })
    }
    fn close(&mut self) -> Result<(), Failure> {
        if !self.deleted {
            if unsafe { DeleteAppContainerProfile(self.name.as_ptr()) } < 0 {
                return Err(Failure::Spawn);
            }
            self.deleted = true;
        }
        Ok(())
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        unsafe {
            if !self.deleted {
                DeleteAppContainerProfile(self.name.as_ptr());
            }
            FreeSid(self.sid);
        }
    }
}

fn grant(path: &Path, sid: PSID, write: bool) -> Result<(), Failure> {
    let name = wide(path);
    let mut old = null_mut();
    let mut descriptor = null_mut();
    let code = unsafe {
        GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            &mut old,
            null_mut(),
            &mut descriptor,
        )
    };
    if code != ERROR_SUCCESS {
        return Err(io_error());
    }
    let mut access: EXPLICIT_ACCESS_W = unsafe { zeroed() };
    access.grfAccessPermissions = if write {
        FILE_ALL_ACCESS
    } else {
        FILE_GENERIC_READ | FILE_GENERIC_EXECUTE
    };
    access.grfAccessMode = GRANT_ACCESS;
    access.grfInheritance = SUB_CONTAINERS_AND_OBJECTS_INHERIT;
    access.Trustee.TrusteeForm = TRUSTEE_IS_SID;
    access.Trustee.TrusteeType = TRUSTEE_IS_UNKNOWN;
    access.Trustee.ptstrName = sid.cast();
    let mut acl = null_mut();
    let result = unsafe { SetEntriesInAclW(1, &access, old, &mut acl) };
    let result = if result == ERROR_SUCCESS {
        unsafe {
            SetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                acl,
                null(),
            )
        }
    } else {
        result
    };
    unsafe {
        LocalFree(descriptor);
        if !acl.is_null() {
            LocalFree(acl.cast());
        }
    }
    if result != ERROR_SUCCESS {
        return Err(io_error());
    }
    Ok(())
}

struct Attributes {
    memory: Vec<usize>,
}
impl Attributes {
    fn new(count: u32) -> Result<Self, Failure> {
        let mut bytes = 0;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), count, 0, &mut bytes);
        }
        let mut memory = vec![0usize; bytes.div_ceil(size_of::<usize>())];
        let ptr = memory.as_mut_ptr().cast();
        if unsafe { InitializeProcThreadAttributeList(ptr, count, 0, &mut bytes) } == 0 {
            return Err(io_error());
        }
        Ok(Self { memory })
    }
    fn ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.memory.as_mut_ptr().cast()
    }
    fn set<T>(&mut self, attribute: u32, value: &T) -> Result<(), Failure> {
        if unsafe {
            UpdateProcThreadAttribute(
                self.ptr(),
                0,
                attribute as usize,
                (value as *const T).cast(),
                size_of::<T>(),
                null_mut(),
                null(),
            )
        } == 0
        {
            return Err(io_error());
        }
        Ok(())
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.ptr());
        }
    }
}

// Windows CRT quoting, with no command interpreter. NUL is rejected before
// construction; backslashes before quotes/the closing quote are doubled.
fn quote(value: &str) -> Result<String, Failure> {
    if value.contains('\0') {
        return Err(Failure::Spawn);
    }
    let mut out = String::from("\"");
    let mut slashes = 0;
    for ch in value.chars() {
        if ch == '\\' {
            slashes += 1;
            continue;
        }
        out.extend(std::iter::repeat('\\').take(if ch == '"' { slashes * 2 + 1 } else { slashes }));
        out.push(ch);
        slashes = 0;
    }
    out.extend(std::iter::repeat('\\').take(slashes * 2));
    out.push('"');
    Ok(out)
}

fn pipe(parent_reads: bool) -> Result<(File, OwnedHandle), Failure> {
    let attrs = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    let (mut read, mut write) = (null_mut(), null_mut());
    if unsafe { CreatePipe(&mut read, &mut write, &attrs, 0) } == 0 {
        return Err(io_error());
    }
    let read = handle(read)?;
    let write = handle(write)?;
    let (parent, child) = if parent_reads {
        (read, write)
    } else {
        (write, read)
    };
    if unsafe { SetHandleInformation(parent.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) } == 0 {
        return Err(io_error());
    }
    Ok((File::from(parent), child))
}

struct Child {
    process: OwnedHandle,
    job: OwnedHandle,
    input: Option<File>,
    output: Option<File>,
    error: Option<File>,
}
impl Process for Child {
    fn stdin(&mut self) -> Option<Box<dyn Write + Send>> {
        self.input.take().map(|f| Box::new(f) as _)
    }
    fn stdout(&mut self) -> Option<Box<dyn Read + Send>> {
        self.output.take().map(|f| Box::new(f) as _)
    }
    fn stderr(&mut self) -> Option<Box<dyn Read + Send>> {
        self.error.take().map(|f| Box::new(f) as _)
    }
    fn poll(&mut self) -> std::io::Result<Option<i32>> {
        let wait = unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) };
        if wait == WAIT_TIMEOUT {
            return Ok(None);
        }
        let mut code = 0;
        if wait != WAIT_OBJECT_0
            || unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &mut code) } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Some(code as i32))
    }
    fn terminate(&mut self) {
        unsafe {
            TerminateJobObject(self.job.as_raw_handle(), 1);
        }
    }
    fn reap(&mut self) {
        unsafe {
            WaitForSingleObject(self.process.as_raw_handle(), 5000);
        }
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        self.terminate();
        self.reap();
    }
}

impl Child {
    fn finish(&mut self) -> Result<(), Failure> {
        self.terminate();
        self.reap();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
            if unsafe {
                QueryInformationJobObject(
                    self.job.as_raw_handle(),
                    JobObjectBasicAccountingInformation,
                    (&mut accounting as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                    size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                    null_mut(),
                )
            } == 0
            {
                return Err(io_error());
            }
            if accounting.ActiveProcesses == 0 {
                return Ok(());
            }
            if std::time::Instant::now() >= until {
                return Err(Failure::Timeout);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

pub(super) fn run(
    command: &transport::AdapterCommand,
    request: &[u8],
    limits: &transport::TransportLimits,
    sandbox: &Sandbox,
    cancel: Option<&AtomicBool>,
) -> Result<transport::TransportSuccess, Failure> {
    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return Err(Failure::Cancelled);
    }
    let mut profile = Profile::new(sandbox)?;
    let result = run_profile(command, request, limits, sandbox, cancel, &profile);
    profile.close()?;
    result
}

fn run_profile(
    command: &transport::AdapterCommand,
    request: &[u8],
    limits: &transport::TransportLimits,
    sandbox: &Sandbox,
    cancel: Option<&AtomicBool>,
    profile: &Profile,
) -> Result<transport::TransportSuccess, Failure> {
    grant(sandbox.owned.path(), profile.sid, false)?;
    // LPAC has low integrity; lower only this owned staged copy so write
    // grants can work without weakening any real-project integrity label.
    let label = wide("S:(ML;OICI;NW;;;LW)");
    let mut descriptor = null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            label.as_ptr(),
            1,
            &mut descriptor,
            null_mut(),
        )
    } == 0
    {
        return Err(io_error());
    }
    let (mut present, mut defaulted, mut sacl) = (0, 0, null_mut());
    let obtained =
        unsafe { GetSecurityDescriptorSacl(descriptor, &mut present, &mut sacl, &mut defaulted) };
    let code = if obtained != 0 {
        unsafe {
            SetNamedSecurityInfoW(
                wide(sandbox.owned.path()).as_ptr(),
                SE_FILE_OBJECT,
                LABEL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                null(),
                sacl,
            )
        }
    } else {
        ERROR_INVALID_SECURITY_DESCR
    };
    unsafe {
        LocalFree(descriptor);
    }
    if code != ERROR_SUCCESS {
        return Err(io_error());
    }
    for root in &sandbox.write_roots {
        grant(root, profile.sid, true)?;
    }
    let (input, child_input) = pipe(false)?;
    let (output, child_output) = pipe(true)?;
    let (error, child_error) = pipe(true)?;
    let handles = [
        child_input.as_raw_handle(),
        child_output.as_raw_handle(),
        child_error.as_raw_handle(),
    ];
    // Node/libuv initializes Winsock even for stdin-only programs. LPAC's
    // registry-read capability permits reading system configuration; it
    // grants neither project-file access nor network connectivity.
    let capabilities = Capabilities::registry_read()?;
    let mut capability_entries: Vec<SID_AND_ATTRIBUTES> = (0..capabilities.count)
        .map(|i| SID_AND_ATTRIBUTES {
            Sid: unsafe { *capabilities.sids.add(i as usize) },
            Attributes: 4, // SE_GROUP_ENABLED
        })
        .collect();
    let security = SECURITY_CAPABILITIES {
        AppContainerSid: profile.sid,
        Capabilities: capability_entries.as_mut_ptr(),
        CapabilityCount: capability_entries.len() as u32,
        Reserved: 0,
    };
    let opt_out = 1u32; // PROCESS_CREATION_ALL_APPLICATION_PACKAGES_OPT_OUT (LPAC).
    let mut attributes = Attributes::new(3)?;
    attributes.set(PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, &security)?;
    attributes.set(
        PROC_THREAD_ATTRIBUTE_ALL_APPLICATION_PACKAGES_POLICY,
        &opt_out,
    )?;
    attributes.set(PROC_THREAD_ATTRIBUTE_HANDLE_LIST, &handles)?;
    let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = handles[0];
    startup.StartupInfo.hStdOutput = handles[1];
    startup.StartupInfo.hStdError = handles[2];
    startup.lpAttributeList = attributes.ptr();
    let executable = wide(&command.program);
    let mut arguments = vec![quote(&command.program.to_string_lossy())?];
    for argument in &command.args {
        arguments.push(quote(argument)?);
    }
    let mut command_line = wide(arguments.join(" "));
    let cwd = wide(&sandbox.project);
    // No inherited user/provider environment, DLL search paths or Node options.
    let system = std::env::var_os("SystemRoot").ok_or(Failure::Spawn)?;
    let private = sandbox.owned.path().to_string_lossy();
    let mut environment: Vec<u16> = Vec::new();
    for (key, value) in [
        ("APPDATA", private.as_ref()),
        ("LOCALAPPDATA", private.as_ref()),
        ("SystemDrive", "C:"),
        ("SystemRoot", system.to_str().ok_or(Failure::Spawn)?),
        ("TEMP", private.as_ref()),
        ("TMP", private.as_ref()),
        ("USERPROFILE", private.as_ref()),
        ("windir", system.to_str().ok_or(Failure::Spawn)?),
    ] {
        environment.extend(wide(format!("{key}={value}")));
    }
    environment.push(0);
    let job = handle(unsafe { CreateJobObjectW(null(), null()) })?;
    let mut job_limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
    job_limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if unsafe {
        SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&job_limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    } == 0
    {
        return Err(io_error());
    }
    let mut info: PROCESS_INFORMATION = unsafe { zeroed() };
    #[cfg(test)]
    transport::record_launch();
    if unsafe {
        CreateProcessW(
            executable.as_ptr(),
            command_line.as_mut_ptr(),
            null(),
            null(),
            1,
            EXTENDED_STARTUPINFO_PRESENT
                | CREATE_SUSPENDED
                | CREATE_NO_WINDOW
                | CREATE_UNICODE_ENVIRONMENT,
            environment.as_ptr().cast::<c_void>(),
            cwd.as_ptr(),
            &startup.StartupInfo,
            &mut info,
        )
    } == 0
    {
        return Err(io_error());
    }
    let process = handle(info.hProcess)?;
    let thread = handle(info.hThread)?;
    if unsafe { AssignProcessToJobObject(job.as_raw_handle(), process.as_raw_handle()) } == 0 {
        unsafe {
            TerminateProcess(process.as_raw_handle(), 1);
            WaitForSingleObject(process.as_raw_handle(), 5000);
        }
        return Err(io_error());
    }
    let mut child = Child {
        process,
        job,
        input: Some(input),
        output: Some(output),
        error: Some(error),
    };
    if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
        return Err(io_error());
    }
    drop(thread);
    drop(child_input);
    drop(child_output);
    drop(child_error);
    let result = transport::pump(&mut child, request, limits, cancel);
    child.finish()?;
    drop(child); // job descendants end before profile/staging cleanup.
    result
}
