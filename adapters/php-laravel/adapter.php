<?php
/**
 * The committed single-file adapter artifact of `lekalo-target-php-laravel`
 * (issues #54, #55).
 *
 * GENERATED FILE — regenerate with `php adapters/php-laravel/build.php`;
 * verify with `php adapters/php-laravel/build.php --check`. Never edit.
 *
 * Self-contained: PHP built-ins only (json, hash, SPL), no Composer
 * package, no extension beyond always-compiled basics, so the confined
 * core runtime can copy the interpreter plus exactly this script.
 */

declare(strict_types=1);
/** Issue #76: static token collection only. Never loads the application. */
function lint_unknown(): array { return ['state' => 'unknown']; }
function lint_known(mixed $value): array { return ['state' => 'known', 'value' => $value]; }
function lint_hash(mixed $value): string { return 'sha256:' . hash('sha256', canonical_json($value)); }
function lint_validate_request(array $request): void
{
    $lint = $request['lint_request'] ?? null;
    if (($request['operation'] === 'lint') !== ($lint !== null)) { throw new RequestRefusal('lint-request', 'pairing'); }
    if ($lint === null) { return; }
    if ($request['protocol_version'] !== '0.6.4' || !isset($request['target'])) { throw new RequestRefusal('lint-request', 'version-target'); }
    foreach (['dry_run','plan_id','native_request','ir_path','profile','profile_digest','profile_capabilities'] as $member) { if (array_key_exists($member,$request)) { throw new RequestRefusal('lint-request','member'); } }
    lint_keys($lint,['bindings','files','pins','scope']);
    foreach (['files'=>4096,'bindings'=>10000,'scope'=>10000] as $key=>$max) { if (!is_array($lint[$key]) || !array_is_list($lint[$key]) || count($lint[$key])>$max) { throw new RequestRefusal('lint-request','limit'); } }
    if (count($lint['scope'])===0) { throw new RequestRefusal('lint-request','scope'); }
    $previous = null;
    foreach ($lint['files'] as $path) { if (!is_string($path) || !is_logical_path($path) || strlen($path)>512 || ($previous!==null && strcmp($previous,$path)>=0)) { throw new RequestRefusal('lint-request','path'); } $previous=$path; }
    $previous = null;
    foreach ($lint['scope'] as $s) { if (!is_string($s) || !preg_match('/^[a-z][a-z0-9_.]{0,190}$/D',$s) || ($previous!==null && strcmp($previous,$s)>=0)) { throw new RequestRefusal('lint-request','scope'); } $previous=$s; }
    lint_keys($lint['pins'],['capabilities','ir','model','observed','profile','revision']);
    foreach ($lint['pins'] as $key=>$pin) {
        if (!is_array($pin) || !isset($pin['state']) || !in_array($pin['state'],['known','unknown','withheld','unsupported'],true)) { throw new RequestRefusal('lint-request','state'); }
        lint_keys($pin,$pin['state']==='known'?['state','value']:['state']);
        if ($pin['state']==='known' && (!is_string($pin['value']) || ($key!=='revision' && !is_sha256_digest($pin['value'])))) { throw new RequestRefusal('lint-request','pin'); }
        if (in_array($key,['model','ir'],true) && $pin['state']!=='known') { throw new RequestRefusal('lint-request','pin'); }
    }
    $seen=[];
    foreach ($lint['bindings'] as $b) { lint_keys($b,['fingerprint','kind','nativeId','path','symbol']); if (!in_array($b['path'],$lint['files'],true) || !in_array($b['kind'],['command','entity'],true) || !is_sha256_digest($b['fingerprint']) || !is_string($b['nativeId']) || !is_string($b['symbol'])) { throw new RequestRefusal('lint-request','binding'); } $key=json_encode([$b['path'],$b['nativeId'],$b['kind']]); if(isset($seen[$key])) {throw new RequestRefusal('lint-request','ambiguous-binding');} $seen[$key]=true; }
}
function lint_keys(mixed $value,array $keys): void { if (!is_array($value)) {throw new RequestRefusal('lint-request','object');} $actual=array_keys($value);sort($actual,SORT_STRING);sort($keys,SORT_STRING);if ($actual!==$keys) {throw new RequestRefusal('lint-request','closed-key');} }
function lint_read_source(string $path): string
{
    if (!is_logical_path($path) || !(str_starts_with($path,'app/') || str_starts_with($path,'src/'))) { throw new RequestRefusal('lint-source','scope'); }
    $root = realpath(getcwd()); $full = realpath($path);
    if ($root===false || $full===false || !str_starts_with(str_replace('\\','/',$full),str_replace('\\','/',$root).'/')) { throw new RequestRefusal('lint-source','escape'); }
    $at='';foreach (explode('/',$path) as $part) { $at=$at===''?$part:$at.'/'.$part; if (is_link($at)) {throw new RequestRefusal('lint-source','link');} }
    $before = @lstat($path);if ($before===false || !is_file($path) || $before['size']>4*1024*1024) {throw new RequestRefusal('lint-source','file');}
    $file = @fopen($path,'rb');if ($file===false) {throw new RequestRefusal('lint-source','read');}
    try { $bytes=stream_get_contents($file,4*1024*1024+1);$after=fstat($file); } finally {fclose($file);}
    if ($bytes===false || strlen($bytes)>4*1024*1024 || $after===false || $before['ino']!==$after['ino'] || $before['dev']!==$after['dev'] || preg_match('//u',$bytes)!==1) {throw new RequestRefusal('lint-source','changed-or-encoding');}
    return $bytes;
}
function lint_collect(array $request): array
{
    $lint=$request['lint_request'];$sources=[];$locations=[];$records=[];$files=[];$bytesTotal=0;$work=0;
    foreach ($lint['files'] as $path) {
        $text=lint_read_source($path);$bytesTotal+=strlen($text);if ($bytesTotal>8*1024*1024) {throw new RequestRefusal('lint-source','limit');}
        $fingerprint='sha256:'.hash('sha256',$text);$source=['id'=>lint_hash([$path,$fingerprint]),'path'=>$path,'fingerprint'=>$fingerprint,'bytes'=>strlen($text)];$sources[]=$source;
        $tokens=[];$offset=0;
        foreach (token_get_all($text) as $token) { $raw=is_array($token)?$token[1]:$token;$kind=is_array($token)?$token[0]:0; if (!in_array($kind,[T_WHITESPACE,T_COMMENT,T_DOC_COMMENT,T_OPEN_TAG,T_CLOSE_TAG],true)) {$tokens[]=['text'=>$raw,'kind'=>$kind,'start'=>$offset,'end'=>$offset+strlen($raw)];} $offset+=strlen($raw);if (++$work>10000000) {throw new RequestRefusal('lint-source','work-limit');} }
        $files[$path]=['text'=>$text,'tokens'=>$tokens,'source'=>$source];
    }
    foreach ($lint['bindings'] as $b) { if (($files[$b['path']]['source']['fingerprint']??null)!==$b['fingerprint']) {throw new RequestRefusal('lint-binding','stale');} }
    $position=static function(string $text,int $byte): array { $pre=substr($text,0,$byte);$parts=explode("\n",$pre);preg_match_all('/./us',end($parts),$chars);return [count($parts),count($chars[0])+1]; };
    $span=static function(string $path,int $start,int $end) use (&$locations,$files,$position): string { $source=$files[$path]['source'];$id=lint_hash([$source['id'],$start,$end]);[$line,$column]=$position($files[$path]['text'],$start);[$endLine,$endColumn]=$position($files[$path]['text'],$end);$locations[$id]=['id'=>$id,'source'=>$source['id'],'start'=>$start,'end'=>$end,'line'=>$line,'column'=>$column,'endLine'=>$endLine,'endColumn'=>$endColumn];return $id; };
    $base=static function(string $kind,string $subject,string $native,array $loc,array $symbol): array {sort($loc,SORT_STRING);return ['id'=>lint_hash([$kind,$subject,$native,$loc]),'kind'=>$kind,'mechanism'=>'direct','subject'=>$subject,'semanticSymbol'=>$symbol,'operation'=>lint_unknown(),'resource'=>lint_unknown(),'field'=>lint_unknown(),'nativeId'=>$native,'confidence'=>'medium','currency'=>'current','origin'=>'extracted','claim'=>'possible-behavior','binding'=>lint_unknown(),'configuration'=>lint_unknown(),'ownership'=>lint_unknown(),'trace'=>lint_unknown(),'value'=>lint_unknown(),'key'=>lint_unknown(),'candidates'=>[],'activation'=>[],'locations'=>$loc,'guards'=>[]];};
    $bindings=[];foreach ($lint['bindings'] as $b) {$bindings[$b['nativeId']]=$b;}
    $classes=[];$methods=[];$registrations=[];
    foreach ($files as $path=>$file) {
        $tokens=$file['tokens'];$n=count($tokens);$class=null;$classDepth=0;$depth=0;$imports=[];
        for ($i=0;$i<$n;$i++) {
            $t=$tokens[$i]['text'];if ($t==='{') {$depth++;}if ($t==='}') {$depth--;if ($class!==null && $depth<$classDepth) {$class=null;}}
            if ($tokens[$i]['kind']===T_USE && $class===null) { $qualified=$tokens[$i+1]['text']??'';$alias=basename(str_replace('\\','/',$qualified));if (($tokens[$i+2]['kind']??null)===T_AS) {$alias=$tokens[$i+3]['text']??$alias;} $imports[$alias]=ltrim($qualified,'\\'); }
            if ($tokens[$i]['kind']===T_CLASS && ($tokens[$i-1]['text']??'')!=='::') {
                $name=$tokens[$i+1]['text']??'';$j=$i+2;while ($j<$n && $tokens[$j]['text']!=='{') {$j++;}
                $baseName=($tokens[$i+2]['kind']??null)===T_EXTENDS?($tokens[$i+3]['text']??''):'';
                $classes[$path.'#'.$name]=['name'=>$name,'native'=>$path.'#'.$name,'eloquent'=>($imports[$baseName]??ltrim($baseName,'\\'))==='Illuminate\\Database\\Eloquent\\Model','methods'=>[]];$class=$path.'#'.$name;$classDepth=$depth+1;
            }
            if ($class!==null && $tokens[$i]['kind']===T_FUNCTION && ($tokens[$i+1]['kind']??null)===T_STRING) {
                $name=$tokens[$i+1]['text'];$j=$i+2;while ($j<$n && $tokens[$j]['text']!=='{') {$j++;}if ($j===$n) {continue;}
                $parameter=[];for ($k=$i+2;$k<$j;$k++) {if ($tokens[$k]['kind']===T_VARIABLE) {$type=$tokens[$k-1]['text']??'';if (isset($classes[$path.'#'.$type]) || isset($bindings[$path.'#'.$type])) {$parameter[$tokens[$k]['text']]=$path.'#'.$type;}}}
                $end=$j+1;$nest=1;while ($end<$n && $nest>0) {if ($tokens[$end]['text']==='{') {$nest++;}if ($tokens[$end]['text']==='}') {$nest--;}$end++;}
                $native=$class.'.'.$name;$method=['name'=>$name,'native'=>$native,'path'=>$path,'class'=>$class,'params'=>$parameter,'from'=>$j+1,'to'=>$end-1,'span'=>$span($path,$tokens[$i]['start'],$tokens[$end-1]['end'])];$methods[$native]=$method;$classes[$class]['methods'][]=$name;
            }
            // Explicit Eloquent class registration, resolved through the class declaration.
            if (($tokens[$i+1]['text']??'')==='::' && ($tokens[$i+2]['text']??'')==='observe' && ($tokens[$i+3]['text']??'')==='(' && ($tokens[$i+5]['text']??'')==='::' && ($tokens[$i+6]['text']??'')==='class') { $registrations[]=['model'=>$path.'#'.$t,'observer'=>$path.'#'.$tokens[$i+4]['text'],'path'=>$path,'span'=>$span($path,$tokens[$i]['start'],$tokens[$i+7]['end'])]; }
        }
    }
    $writes=static function(array $method) use ($files,$bindings,$span): array { $out=[];$tokens=$files[$method['path']]['tokens'];for ($i=$method['from'];$i<$method['to']-3;$i++) { $parameter=$method['params'][$tokens[$i]['text']]??null;$bound=$bindings[$parameter]??null;if ($parameter!==null && $bound!==null && $bound['kind']==='entity' && ($tokens[$i+1]['text']??'')==='->' && ($tokens[$i+2]['kind']??null)===T_STRING && ($tokens[$i+3]['text']??'')==='=') {$out[]=['resource'=>$bound['symbol'],'field'=>$tokens[$i+2]['text'],'span'=>$span($method['path'],$tokens[$i]['start'],$tokens[$i+3]['end'])];} }return $out; };
    foreach ($methods as $method) {
        $bound=$bindings[$method['native']]??null;$symbol=$bound!==null?lint_known($bound['symbol']):lint_unknown();$subject=$bound['symbol']??$method['native'];
        if ($bound!==null && !array_filter($lint['scope'],static fn($s)=>$s===$bound['symbol'] || str_starts_with($bound['symbol'],$s.'.'))) {continue;}
        if ($bound!==null && $bound['kind']==='command') {foreach ($writes($method) as $write) {$r=$base('field-write',$subject,$method['native'],[$write['span']],$symbol);$r['operation']=$symbol;$r['resource']=lint_known($write['resource']);$r['field']=lint_known($write['field']);$r['key']=lint_known('update');$r['confidence']='high';$r['claim']='structural';$records[$r['id']]=$r;}}
        $tokens=$files[$method['path']]['tokens'];
        for ($i=$method['from'];$i<$method['to']-3;$i++) {
            if (++$work>10000000) {throw new RequestRefusal('lint-source','work-limit');}
            if (($tokens[$i+1]['text']??'')==='->' && (in_array($tokens[$i+2]['kind']??0,[T_VARIABLE],true) || ($tokens[$i+2]['text']??'')==='{')) { $r=$base('reflection',$subject,$method['native'].':dynamic-call',[$span($method['path'],$tokens[$i]['start'],$tokens[$i+2]['end'])],$symbol);$r['binding']=lint_known(false);$r['guards']=['computed-method-name'];$records[$r['id']]=$r; }
            $nativeModel=$method['params'][$tokens[$i]['text']]??null;$model=$classes[$nativeModel]??null;
            if ($bound===null || $model===null || !$model['eloquent'] || in_array('save',$model['methods'],true) || in_array('observe',$model['methods'],true) || ($tokens[$i+1]['text']??'')!=='->' || ($tokens[$i+2]['text']??'')!=='save' || ($tokens[$i+3]['text']??'')!=='(') {continue;}
            foreach ($registrations as $reg) { if ($reg['model']!==$nativeModel) {continue;} $callback=$methods[$reg['observer'].'.saved']??null;if ($callback===null) {continue;}
                foreach ($writes($callback) as $write) {
                    $trigger=$span($method['path'],$tokens[$i]['start'],$tokens[$i+4]['end']);$ids=[$trigger,$reg['span'],$callback['span'],$write['span']];$ids=array_values(array_unique($ids));$r=$base('effect',$subject,$method['native'].':observer:'.$reg['observer'],$ids,$symbol);$r['operation']=$symbol;$r['resource']=lint_known($write['resource']);$r['field']=lint_known($write['field']);$r['key']=lint_known('update');$r['mechanism']='observer';$r['confidence']='high';
                    foreach ([['binding',$method['native'],$trigger],['trigger',$method['native'].':save',$trigger],['registration',$reg['observer'],$reg['span']],['callback',$callback['native'],$callback['span']],['effect',$write['resource'].'/'.$write['field'],$write['span']]] as [$role,$identity,$location]) {$r['activation'][]=['role'=>$role,'identity'=>$identity,'locations'=>[$location],'guards'=>['observer-still-registered','save-succeeds','framework-event-semantics'],'confidence'=>'high'];}
                    $records[$r['id']]=$r;
                }
            }
        }
    }
    if (count($records)>10000 || count($locations)>10000) {throw new RequestRefusal('lint-source','record-limit');}
    usort($sources,static fn($a,$b)=>strcmp($a['path'],$b['path']));ksort($locations,SORT_STRING);ksort($records,SORT_STRING);
    $rules=['ambiguity.implicit-target-defaults','ambiguity.multiple-resolutions','ambiguity.scattered-state-writes','hidden.convention-only-path','hidden.dispatch-without-binding','hidden.observer-write','hidden.path-without-trace-owner','hidden.reflective-call','hidden.string-reference','hidden.undeclared-effect','indirection.depth-exceeded'];$coverage=[];
    foreach ($rules as $rule) {foreach ($lint['scope'] as $scope) {$supported=in_array($rule,['hidden.observer-write','hidden.reflective-call','ambiguity.scattered-state-writes'],true);$coverage[]=['rule'=>$rule,'target'=>$request['target'],'scope'=>$scope,'state'=>$supported&&count($files)>0?'partial':'unsupported','eligible'=>lint_unknown(),'examined'=>lint_known(count($methods)),'limitations'=>[$supported?'bounded-token-files':'detector-unsupported']];}}
    return ['schemaVersion'=>'lekalo/ai-lint-evidence/v0.6.4','identity'=>'dev.lekalo.ai-lint-evidence@0.6.4','target'=>$request['target'],'scope'=>$lint['scope'],'producer'=>['id'=>ADAPTER_ID,'version'=>ADAPTER_VERSION,'artifactDigest'=>'sha256:'.hash_file('sha256', __FILE__),'tool'=>'php-token-static','compiler'=>lint_known('php/'.PHP_MAJOR_VERSION.'.'.PHP_MINOR_VERSION),'framework'=>lint_unknown(),'recipe'=>'ai-readability/1'],'pins'=>$lint['pins'],'inputManifestDigest'=>lint_hash($sources),'sources'=>$sources,'locations'=>array_values($locations),'records'=>array_values($records),'coverage'=>$coverage,'limitations'=>['static-files-only','no-application-execution','framework-version-unknown','external-class-resolution-unsupported']];
}

// ---- bundled toolchain lock (issue #55) -------------------------
// EMBEDDED BY build.php from mago-toolchain.lock.json. Never edit.
// The exact bytes are part of this artifact, so the artifact digest
// changes whenever the supported toolchain changes (custody binds).
const MAGO_TOOLCHAIN_LOCK_BUNDLED = '{
  "schemaVersion": "lekalo/mago-toolchain-lock/v0.1.0",
  "identity": "dev.lekalo.mago-toolchain@0.1.0",
  "tool": {
    "name": "mago",
    "vendor": "carthage-software",
    "upstream": "https://github.com/carthage-software/mago",
    "license": "MIT OR Apache-2.0",
    "version": "1.0.0"
  },
  "probe": {
    "version": {
      "argv": ["mago", "--version"],
      "stdout": "mago 1.0.0"
    },
    "artifacts": [
      {
        "platform": "x86_64-pc-windows-msvc",
        "archive": "mago-1.0.0-x86_64-pc-windows-msvc.zip",
        "archiveSha256": "db898a5eea4b13529f202fa5968d2a17643b9c7109a5705001f466fddc07156b",
        "binary": "mago.exe",
        "binarySha256": "4702d8c00acb73f23624fa17ba771f5d0e060bb31f299d5b5dae49ece6f845fa"
      },
      {
        "platform": "x86_64-unknown-linux-gnu",
        "archive": "mago-1.0.0-x86_64-unknown-linux-gnu.tar.gz",
        "archiveSha256": "ea012e30d3c9f7b899bf6e2d3feaf61091c931bc3af248519fb01f2ff2375be4",
        "binary": "mago",
        "binarySha256": "582646d691f3307caf47cf23d6009aba949078ecc899bafd22c04a1634c185db"
      }
    ]
  },
  "compatibility": {
    "decoderRevision": "v1",
    "phpSyntax": [
      "7.2",
      "7.3",
      "7.4",
      "8.0",
      "8.1",
      "8.2",
      "8.3",
      "8.4",
      "8.5"
    ],
    "capabilities": {
      "syntaxAst": "full",
      "lint": "full",
      "analyze": "full",
      "guard": "full",
      "typeInference": "partial",
      "references": "partial",
      "incremental": "unknown",
      "fixPreview": "text-only",
      "fixApply": "out-of-scope"
    },
    "exitCodes": {
      "ok": 0,
      "findingsOrInfrastructure": 1,
      "usageOrConfiguration": 2
    }
  },
  "commands": {
    "lintRules": {
      "argv": ["mago", "lint", "--list-rules", "--json"],
      "exitCodes": [0]
    },
    "lintJson": {
      "argv": [
        "mago", "--workspace", ".", "lint",
        "--reporting-format", "json", "--reporting-target", "stdout"
      ],
      "exitCodes": [0, 1],
      "notes": "exit 0 = clean or warning-only (default minimum-fail-level error); exit 1 = error-level findings or infrastructure failure; exit 2 = usage/config error"
    },
    "lintSarif": {
      "argv": [
        "mago", "--workspace", ".", "lint",
        "--reporting-format", "sarif", "--reporting-target", "stdout"
      ],
      "exitCodes": [0, 1]
    },
    "analyzeJson": {
      "argv": [
        "mago", "--workspace", ".", "analyze",
        "--reporting-format", "json", "--reporting-target", "stdout"
      ],
      "exitCodes": [0, 1]
    },
    "analyzeSarif": {
      "argv": [
        "mago", "--workspace", ".", "analyze",
        "--reporting-format", "sarif", "--reporting-target", "stdout"
      ],
      "exitCodes": [0, 1]
    },
    "guardJson": {
      "argv": [
        "mago", "--workspace", ".", "guard",
        "--reporting-format", "json", "--reporting-target", "stdout"
      ],
      "exitCodes": [0, 1]
    },
    "guardSarif": {
      "argv": [
        "mago", "--workspace", ".", "guard",
        "--reporting-format", "sarif", "--reporting-target", "stdout"
      ],
      "exitCodes": [0, 1]
    },
    "astJson": {
      "argv": ["mago", "--workspace", ".", "ast", "--json", "<FILE>"],
      "exitCodes": [0],
      "notes": "creates .mago cache; not used by the adapter"
    },
    "fixPreview": {
      "argv": [
        "mago", "--workspace", ".", "lint",
        "--fix", "--dry-run", "--unsafe", "--only", "<RULE>"
      ],
      "exitCodes": [0, 1],
      "notes": "unified diff on stdout; requires --unsafe because the strict-types insertion fix is classified potentially-unsafe; never combined with --reporting-format (clap refuses that pair)"
    }
  }
}
';

/**
 * The external-analyzer seam of the PHP kernel (issue #55).
 *
 * The kernel itself stays subprocess-free: it never launches Mago, reads
 * a PATH, or runs Composer. Instead, a core-owned runner may produce an
 * *analyzer receipt* (`.lekalo/import/mago/receipt.json`) describing one
 * bounded Mago run; the kernel consumes that receipt through this closed
 * `Analyzer` contract. Two implementations exist:
 *
 * - `MagoEvidenceAnalyzer`: strict decoder/validation of a runner
 *   receipt; refuses missing, stale, mismatched, or malformed evidence
 *   with an explicit state instead of a fake success.
 * - `FakeAnalyzer`: a deterministic test double used by the suites and
 *   the fake integration gate; its canned output traverses exactly the
 *   same decoder, so a fake can never diverge from real semantics.
 *
 * Closed analysis states (the issue requires `Mago unavailable` to be
 * distinct from `analysis failure`):
 *   unavailable   — no receipt exists (tool never ran; absent capability)
 *   incompatible  — receipt exists, but tool version/decoder revision
 *                   differs from the pinned toolchain lock
 *   failed        — the recorded run itself failed (nonzero completion,
 *                   parse errors, or refused output)
 *   ok            — a well-formed, current, successful receipt
 *
 * (No module-level declare: the artifact's single strict_types
 * declaration at the top of the bundled file covers every module.)
 */

/** The receipt schema this kernel decodes (closed; bump on change). */
const MAGO_RECEIPT_SCHEMA = 'lekalo/provider-evidence/v0.1.0';
/** The pinned toolchain identity the receipt must agree with. */
const MAGO_TOOLCHAIN_LOCK_FILE = 'mago-toolchain.lock.json';
/** The pinned upstream tool version the decoder speaks (build-checked). */
const MAGO_PINNED_TOOL_VERSION = '1.0.0';
const MAGO_RECEIPT_PATH = '.lekalo/import/mago/receipt.json';
/** The maximum receipt document size (mirrors the core import bounds). */
const MAGO_RECEIPT_MAX_BYTES = 1024 * 1024;
/** The maximum number of diagnostics and symbols one receipt may carry. */
const MAGO_RECEIPT_MAX_ITEMS = 256;

/** The closed analysis states. */
const MAGO_STATES = ['ok', 'unavailable', 'incompatible', 'failed'];

/**
 * The analysis outcome handed to dispatch; every state other than `ok`
 * carries a bounded, machine-readable reason.
 */
final class AnalysisOutcome
{
    /** @param array<int, array<string, mixed>> $diagnostics */
    public function __construct(
        public readonly string $state,
        public readonly array $diagnostics = [],
        public readonly array $symbols = [],
        public readonly array $relations = [],
        public readonly array $fixes = [],
        public readonly ?string $reason = null,
        public readonly ?string $receiptDigest = null,
    ) {
    }

    public function isOk(): bool
    {
        return $this->state === 'ok';
    }
}

/** Why a receipt was refused. */
final class ReceiptRefusal extends RuntimeException
{
}

/** The closed analyzer contract; injection happens at composition only. */
interface Analyzer
{
    /** Capability identity: which receipt schema and pin this analyzer reads. */
    public function capabilities(): array;

    /** Produce one outcome for the staged read view (never launches anything). */
    public function analyze(): AnalysisOutcome;
}

/**
 * The production analyzer: decode and validate the runner receipt inside
 * the declared read view. Missing, incompatible, and failed receipts are
 * explicit outcomes — never collapsed into an empty success.
 */
final class MagoEvidenceAnalyzer implements Analyzer
{
    public function __construct()
    {
    }

    public function capabilities(): array
    {
        return [
            'analyzer' => 'mago',
            'receipt_schema' => MAGO_RECEIPT_SCHEMA,
            'toolchain_lock_digest' => mago_load_toolchain_lock()['lockDigest']
                ?? ('sha256:' . str_repeat('0', 64)),
            'modes' => ['lint', 'analyze', 'guard'],
        ];
    }

    public function analyze(): AnalysisOutcome
    {
        if (!is_file(MAGO_RECEIPT_PATH)) {
            return new AnalysisOutcome(
                'unavailable',
                reason: 'no analyzer receipt at ' . MAGO_RECEIPT_PATH,
            );
        }
        try {
            $receipt = mago_decode_receipt(
                (string) file_get_contents(MAGO_RECEIPT_PATH),
            );
        } catch (ReceiptRefusal $refusal) {
            return new AnalysisOutcome(
                'failed',
                reason: 'receipt refused: ' . $refusal->getMessage(),
            );
        }
        $compat = mago_check_compatibility($receipt);
        if ($compat !== null) {
            return new AnalysisOutcome('incompatible', reason: $compat);
        }
        if (($receipt['completion']['status'] ?? '') !== 'completed') {
            return new AnalysisOutcome(
                'failed',
                reason: 'recorded run did not complete: '
                    . (string) ($receipt['completion']['status'] ?? 'missing'),
                receiptDigest: $receipt['receipt_digest'] ?? null,
            );
        }
        return new AnalysisOutcome(
            'ok',
            diagnostics: $receipt['diagnostics'],
            symbols: $receipt['symbols'],
            relations: $receipt['relations'],
            fixes: $receipt['fixes'],
            receiptDigest: isset($receipt['receipt_digest']) ? (string) $receipt['receipt_digest'] : null,
        );
    }
}

/**
 * The deterministic test double used by the default production dispatch
 * (and the suites). The default dispatch has no staged receipt and must
 * preserve the #54 no-evidence, no-claims behavior, so the composition
 * default is an explicitly `unavailable` analyzer: no canned rows, and
 * the state is honest absence rather than a failed decode.
 */
final class FakeAnalyzer implements Analyzer
{
    /** @param array<string, mixed> $canned */
    public function __construct(
        private readonly array $canned = [],
        private readonly string $state = 'unavailable',
    ) {
    }

    public function capabilities(): array
    {
        return [
            'analyzer' => 'fake',
            'receipt_schema' => MAGO_RECEIPT_SCHEMA,
            'toolchain_lock_digest' => 'sha256:' . str_repeat('0', 64),
            'modes' => ['lint', 'analyze', 'guard'],
        ];
    }

    public function analyze(): AnalysisOutcome
    {
        if ($this->state !== 'ok') {
            return new AnalysisOutcome($this->state, reason: 'fake analyzer canned state');
        }
        try {
            $receipt = mago_decode_receipt(
                json_encode($this->canned, JSON_THROW_ON_ERROR),
            );
        } catch (ReceiptRefusal $refusal) {
            return new AnalysisOutcome(
                'failed',
                reason: 'fake receipt refused: ' . $refusal->getMessage(),
            );
        }
        // The fake traverses the same semantic gates as the production
        // consumer: compatibility against the bundled/file lock and the
        // completion-status gate — so a canned ok receipt with a failed
        // completion or a foreign tool digest reports failed or
        // incompatible exactly like the real one.
        $compat = mago_check_compatibility($receipt);
        if ($compat !== null) {
            return new AnalysisOutcome('incompatible', reason: $compat);
        }
        if (($receipt['completion']['status'] ?? '') !== 'completed') {
            return new AnalysisOutcome(
                'failed',
                reason: 'recorded run did not complete: '
                    . (string) ($receipt['completion']['status'] ?? 'missing'),
            );
        }
        return new AnalysisOutcome(
            'ok',
            diagnostics: $receipt['diagnostics'],
            symbols: $receipt['symbols'],
            relations: $receipt['relations'],
            fixes: $receipt['fixes'],
            receiptDigest: isset($receipt['receipt_digest']) ? (string) $receipt['receipt_digest'] : null,
        );
    }
}
// ---------------------------------------------------------------------------
// Receipt decoding: the strict, closed, bounded decoder.
// ---------------------------------------------------------------------------

/**
 * Decode one receipt document with the kernel's fatal conventions:
 * malformed UTF-8, duplicate keys, unknown members, null members, bound
 * overflow, and digest mismatches are refusals — never silent drops.
 *
 * @return array<string, mixed>
 */
function mago_decode_receipt(string $bytes): array
{
    if ($bytes === '') {
        throw new ReceiptRefusal('empty');
    }
    if (strlen($bytes) > MAGO_RECEIPT_MAX_BYTES) {
        throw new ReceiptRefusal('receipt-too-large');
    }
    if (!preg_match('//u', $bytes)) {
        throw new ReceiptRefusal('utf-8');
    }
    reject_duplicate_keys($bytes);
    try {
        $value = json_decode($bytes, true, 32, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        throw new ReceiptRefusal('syntax');
    }
    if (!is_json_object($value)) {
        throw new ReceiptRefusal('shape');
    }
    $keys = [
        'schema', 'receipt_digest', 'tool', 'completion', 'input_manifest',
        'diagnostics', 'symbols', 'relations', 'fixes',
    ];
    foreach (array_keys($value) as $key) {
        if (!in_array($key, $keys, true)) {
            throw new ReceiptRefusal('unknown-key:' . $key);
        }
        if ($value[$key] === null) {
            throw new ReceiptRefusal('null-member:' . $key);
        }
    }
    foreach (['schema', 'receipt_digest', 'tool', 'completion', 'input_manifest'] as $key) {
        if (!array_key_exists($key, $value)) {
            throw new ReceiptRefusal('missing-key:' . $key);
        }
    }
    if ($value['schema'] !== MAGO_RECEIPT_SCHEMA) {
        throw new ReceiptRefusal('schema');
    }
    if (!is_sha256_digest($value['receipt_digest'])) {
        throw new ReceiptRefusal('receipt-digest');
    }
    if (!is_json_object($value['tool']) || !is_json_object($value['completion'])
        || !is_json_object($value['input_manifest'])) {
        throw new ReceiptRefusal('shape');
    }
    foreach ($value['tool'] as $key => $_) {
        if (!in_array($key, ['name', 'version', 'digest'], true)) {
            throw new ReceiptRefusal('unknown-key:tool.' . $key);
        }
    }
    if (($value['tool']['name'] ?? '') !== 'mago' || !is_string($value['tool']['version'])
        || $value['tool']['version'] === '' || strlen((string) $value['tool']['version']) > 32
        || !is_sha256_digest($value['tool']['digest'] ?? null)) {
        throw new ReceiptRefusal('tool');
    }
    foreach ($value['completion'] as $key => $_) {
        if (!in_array($key, ['status', 'exit_code'], true)) {
            throw new ReceiptRefusal('unknown-key:completion.' . $key);
        }
    }
    if (!in_array($value['completion']['status'] ?? '', ['completed', 'failed'], true)) {
        throw new ReceiptRefusal('completion.status');
    }
    foreach ($value['input_manifest'] as $key => $_) {
        if (!in_array($key, ['inputs', 'source_digest'], true)) {
            throw new ReceiptRefusal('unknown-key:input_manifest.' . $key);
        }
    }
    if (!is_sha256_digest($value['input_manifest']['source_digest'] ?? null)) {
        throw new ReceiptRefusal('input_manifest.source_digest');
    }
    foreach (['diagnostics', 'symbols', 'relations', 'fixes'] as $key) {
        $items = $value[$key] ?? [];
        if ($items === []) {
            $value[$key] = [];
            continue;
        }
        if (!is_array($items) || !array_is_list($items)) {
            throw new ReceiptRefusal($key . ':shape');
        }
        if (count($items) > MAGO_RECEIPT_MAX_ITEMS) {
            throw new ReceiptRefusal($key . ':overflow');
        }
    }
    foreach (($value['diagnostics'] ?? []) as $index => $item) {
        $value['diagnostics'][$index] = mago_decode_diagnostic($item);
    }
    foreach (($value['symbols'] ?? []) as $index => $item) {
        $value['symbols'][$index] = mago_decode_symbol($item);
    }
    foreach (($value['relations'] ?? []) as $index => $item) {
        $value['relations'][$index] = mago_decode_relation($item);
    }
    foreach (($value['fixes'] ?? []) as $index => $item) {
        $value['fixes'][$index] = mago_decode_fix($item);
    }
    return $value;
}

/**
 * One normalized diagnostic row: registered Lekalo rule id, bounded
 * logical path, half-open range, namespaced original code, and the
 * bounded payload. Unknown upstream codes keep their exact original
 * code under the generic native-finding rule.
 *
 * Laravel casing: the staged source files under `app/` keep their
 * canonical casing (e.g. `app/Models/User.php`), which the v0.3.2
 * logical-path grammar cannot spell. Findings for target sources are
 * validated as *native evidence paths* — closed, traversal-free, and
 * case-preserving — not as Model logical paths.
 *
 * @return array<string, mixed>
 */
function mago_decode_diagnostic(mixed $item): array
{
    if (!is_json_object($item)) {
        throw new ReceiptRefusal('diagnostic:shape');
    }
    foreach (array_keys($item) as $key) {
        if (!in_array($key, ['rule', 'original_code', 'level', 'path', 'range', 'message', 'producer'], true)) {
            throw new ReceiptRefusal('diagnostic:unknown-key:' . $key);
        }
    }
    foreach (['rule', 'original_code', 'level', 'path', 'range', 'producer'] as $key) {
        if (!array_key_exists($key, $item)) {
            throw new ReceiptRefusal('diagnostic:missing:' . $key);
        }
        if ($item[$key] === null) {
            throw new ReceiptRefusal('diagnostic:null:' . $key);
        }
    }
    if (!is_analysis_rule_id($item['rule'])) {
        throw new ReceiptRefusal('diagnostic:rule');
    }
    $code = $item['original_code'];
    if (!is_string($code) || $code === '' || strlen($code) > 64
        || (bool) preg_match('/[\x00-\x1f]/', $code)) {
        throw new ReceiptRefusal('diagnostic:original-code');
    }
    if (!in_array($item['level'], ['note', 'help', 'warning', 'error'], true)) {
        throw new ReceiptRefusal('diagnostic:level');
    }
    if (!is_native_evidence_path($item['path'])) {
        throw new ReceiptRefusal('diagnostic:path');
    }
    $range = $item['range'];
    if (!is_json_object($range)
        || !isset($range['start'], $range['end'])
        || !is_int($range['start']) || !is_int($range['end'])
        || $range['start'] < 0 || $range['end'] < $range['start']
        || $range['end'] - $range['start'] > MAGO_RECEIPT_MAX_BYTES) {
        throw new ReceiptRefusal('diagnostic:range');
    }
    if (array_key_exists('message', $item)
        && (!is_string($item['message']) || strlen($item['message']) > 512)) {
        throw new ReceiptRefusal('diagnostic:message');
    }
    if (!in_array($item['producer'], ['mago', 'lekalo'], true)) {
        throw new ReceiptRefusal('diagnostic:producer');
    }
    return $item;
}

/**
 * One symbol row: package-qualified identity, kind, span, and the
 * structural signature digest. Line movement never changes identity.
 *
 * @return array<string, mixed>
 */
function mago_decode_symbol(mixed $item): array
{
    if (!is_json_object($item)) {
        throw new ReceiptRefusal('symbol:shape');
    }
    foreach (array_keys($item) as $key) {
        if (!in_array($key, ['identity', 'kind', 'path', 'range', 'signature', 'modifiers'], true)) {
            throw new ReceiptRefusal('symbol:unknown-key:' . $key);
        }
    }
    foreach (['identity', 'kind', 'path', 'range'] as $key) {
        if (!array_key_exists($key, $item)) {
            throw new ReceiptRefusal('symbol:missing:' . $key);
        }
    }
    if (!is_symbol_identity($item['identity'])) {
        throw new ReceiptRefusal('symbol:identity');
    }
    if (!in_array($item['kind'], ['class', 'interface', 'trait', 'enum', 'function', 'method', 'property'], true)) {
        throw new ReceiptRefusal('symbol:kind');
    }
    if (!is_native_evidence_path($item['path'])) {
        throw new ReceiptRefusal('symbol:path');
    }
    $range = $item['range'];
    if (!is_json_object($range) || !isset($range['start'], $range['end'])
        || !is_int($range['start']) || !is_int($range['end'])
        || $range['start'] < 0 || $range['end'] < $range['start']) {
        throw new ReceiptRefusal('symbol:range');
    }
    if (array_key_exists('signature', $item)
        && (!is_string($item['signature']) || !is_sha256_digest($item['signature']))) {
        throw new ReceiptRefusal('symbol:signature');
    }
    if (array_key_exists('modifiers', $item)) {
        if (!is_array($item['modifiers']) || !array_is_list($item['modifiers'])) {
            throw new ReceiptRefusal('symbol:modifiers');
        }
        foreach ($item['modifiers'] as $modifier) {
            if (!in_array($modifier, ['final', 'readonly', 'abstract', 'static', 'public', 'protected', 'private', 'extensible'], true)) {
                throw new ReceiptRefusal('symbol:modifier');
            }
        }
    }
    return $item;
}

/**
 * One relation row: typed endpoint pair with explicit confidence and
 * provenance. Unresolved endpoints are refused here; uncertainty is
 * expressed with confidence `unknown`, never with invented targets.
 *
 * @return array<string, mixed>
 */
function mago_decode_relation(mixed $item): array
{
    if (!is_json_object($item)) {
        throw new ReceiptRefusal('relation:shape');
    }
    foreach (array_keys($item) as $key) {
        if (!in_array($key, ['from', 'to', 'role', 'confidence', 'producer', 'derivation'], true)) {
            throw new ReceiptRefusal('relation:unknown-key:' . $key);
        }
    }
    foreach (['from', 'to', 'role', 'confidence', 'producer'] as $key) {
        if (!array_key_exists($key, $item)) {
            throw new ReceiptRefusal('relation:missing:' . $key);
        }
    }
    foreach (['from', 'to'] as $key) {
        if (!is_symbol_identity($item[$key])) {
            throw new ReceiptRefusal('relation:' . $key);
        }
    }
    if (!in_array($item['role'], ['extends', 'implements', 'uses', 'calls', 'reads', 'writes', 'instantiates', 'relation', 'route', 'container-binding'], true)) {
        throw new ReceiptRefusal('relation:role');
    }
    if (!in_array($item['confidence'], ['exact', 'high', 'medium', 'low', 'unknown'], true)) {
        throw new ReceiptRefusal('relation:confidence');
    }
    if (!in_array($item['producer'], ['mago', 'laravel-extension', 'lekalo'], true)) {
        throw new ReceiptRefusal('relation:producer');
    }
    if (array_key_exists('derivation', $item)
        && (!is_string($item['derivation']) || strlen($item['derivation']) > 128)) {
        throw new ReceiptRefusal('relation:derivation');
    }
    return $item;
}

/**
 * One safe-fix record: advice only. A fix never carries raw source
 * patches across the boundary — only ranges, replacements, hashes, and
 * the conservative safety classification.
 *
 * @return array<string, mixed>
 */
function mago_decode_fix(mixed $item): array
{
    if (!is_json_object($item)) {
        throw new ReceiptRefusal('fix:shape');
    }
    foreach (array_keys($item) as $key) {
        if (!in_array($key, ['rule', 'path', 'range', 'replacement', 'before_digest', 'safety'], true)) {
            throw new ReceiptRefusal('fix:unknown-key:' . $key);
        }
    }
    foreach (['rule', 'path', 'range', 'before_digest', 'safety'] as $key) {
        if (!array_key_exists($key, $item)) {
            throw new ReceiptRefusal('fix:missing:' . $key);
        }
    }
    if (!is_string($item['rule']) || !is_analysis_rule_id($item['rule'])
        && !preg_match('/^[a-z0-9][a-z0-9.-]{0,126}[a-z0-9]$/', $item['rule'])) {
        throw new ReceiptRefusal('fix:rule');
    }
    if (!is_native_evidence_path($item['path'])) {
        throw new ReceiptRefusal('fix:path');
    }
    $range = $item['range'];
    if (!is_json_object($range) || !isset($range['start'], $range['end'])
        || !is_int($range['start']) || !is_int($range['end'])
        || $range['start'] < 0 || $range['end'] < $range['start']) {
        throw new ReceiptRefusal('fix:range');
    }
    if (!is_sha256_digest($item['before_digest'])) {
        throw new ReceiptRefusal('fix:before-digest');
    }
    if (!in_array($item['safety'], ['safe', 'potentially-unsafe', 'unsafe'], true)) {
        throw new ReceiptRefusal('fix:safety');
    }
    if (array_key_exists('replacement', $item)
        && (!is_string($item['replacement']) || strlen($item['replacement']) > 4096)) {
        throw new ReceiptRefusal('fix:replacement');
    }
    return $item;
}

/**
 * Compatibility gate: the receipt must agree with the pinned toolchain
 * (tool name/version/digest and the decoder revision the kernel speaks).
 * Returns null when compatible, or the bounded incompatibility reason.
 */
function mago_check_compatibility(array $receipt): ?string
{
    // The lock is the bundled custody source; there is no second digest
    // input to race against (both the analyzer and the fake resolve the
    // same cached load), so the receipt is compared against the pin
    // directly. The artifact digest moving with the lock is the upgrade
    // gate — a rebuilt artifact carries the new pin by construction.
    $lock = mago_load_toolchain_lock();
    if ($lock === null) {
        return 'toolchain lock missing';
    }
    // The lock stores bare hex; the receipt carries the `sha256:`
    // spelling — compare against both so custody compares the bytes.
    $digestMatch = false;
    foreach (($lock['probe']['artifacts'] ?? []) as $artifact) {
        if (in_array($receipt['tool']['digest'] ?? '', [
            'sha256:' . ($artifact['binarySha256'] ?? ''),
            $artifact['binarySha256'] ?? '',
        ], true)) {
            $digestMatch = true;
            break;
        }
    }
    if (!$digestMatch) {
        return 'tool digest differs from the pinned toolchain';
    }
    if (($receipt['tool']['version'] ?? '') !== ($lock['tool']['version'] ?? '')) {
        return 'tool version differs from the pinned toolchain';
    }
    return null;
}

/**
 * Load the adapter-owned toolchain lock (bundled beside the kernel at
 * build time); returns null when the packaging did not embed one.
 *
 * @return array<string, mixed>|null
 */
function mago_load_toolchain_lock(): ?array
{
    static $cache = false;
    static $lock = null;
    if ($cache === true) {
        return $lock;
    }
    $cache = true;
    // Custody source of truth: the verbatim lock bytes bundled into the
    // artifact by build.php (MAGO_TOOLCHAIN_LOCK_BUNDLED). The lock is
    // part of the shipped bytes, so the deployed single-file package
    // carries its own custody and no sibling file is needed. In the
    // source tree (before bundling) the constant does not exist yet;
    // dev/test then reads the same lock file beside src/ — the exact
    // bytes build.php embeds.
    if (defined('MAGO_TOOLCHAIN_LOCK_BUNDLED')) {
        $bytes = (string) MAGO_TOOLCHAIN_LOCK_BUNDLED;
    } else {
        $path = dirname(__DIR__) . '/mago-toolchain.lock.json';
        if (!is_file($path)) {
            return null;
        }
        $bytes = (string) file_get_contents($path);
    }
    try {
        $lock = json_decode($bytes, true, 32, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return $lock = null;
    }
    if (!is_array($lock) || !is_json_object($lock)) {
        return $lock = null;
    }
    $lock['lockDigest'] = sha256_digest($bytes);
    return $lock;
}

/**
 * The Lekalo strict-profile rules over the analyzer receipt (issue #55).
 *
 * The issue's target table maps each required rule to its evidence
 * source. Two implementation shapes exist and are kept strictly apart:
 *
 * - `lint` rows: a pinned Mago rule supplies the finding verbatim; the
 *   kernel only maps the original code into a registered rule id and
 *   keeps the exact original code in namespaced metadata.
 * - `predicate` rows: no upstream rule exists for the requirement, so
 *   the kernel evaluates a deterministic predicate over the receipt's
 *   symbol/AST evidence. When the prerequisite evidence is absent the
 *   row is `unsupported` — never a silent pass.
 *
 * Every row: compliant inputs produce no finding; violating inputs
 * produce one; evidence that cannot decide produces an explicit
 * `target.analysis.evidence-unsupported` diagnostic instead of a
 * fabricated pass or fail.
 */

/** The closed strict-profile row set (ids are kernel-stable). */
const STRICT_RULES = [
    'strict-types' => [
        'rule' => 'target.analysis.strict-types',
        'source' => 'lint',
        'mago_code' => 'strict-types',
    ],
    'final-readonly-profile' => [
        'rule' => 'target.analysis.final-readonly-profile',
        'source' => 'predicate',
    ],
    'no-dynamic-members' => [
        'rule' => 'target.analysis.no-dynamic-members',
        'source' => 'lint',
        'mago_code' => 'no-variable-variable',
    ],
    'no-service-locator' => [
        'rule' => 'target.analysis.no-service-locator',
        'source' => 'predicate',
    ],
    'no-magic-domain-state' => [
        'rule' => 'target.analysis.no-magic-domain-state',
        'source' => 'predicate',
    ],
    // Explicit nullable/array-shape/type coverage has no pinned upstream
    // rule AND no public shape-inference evidence in the receipt, so the
    // honest state is `unsupported` — never a re-mapped copy of the
    // strict-types diagnostics (which would double-report one rule).
    'explicit-types' => [
        'rule' => 'target.analysis.explicit-types',
        'source' => 'predicate',
    ],
];

/**
 * Evaluate the whole strict profile over one `ok` outcome. Returns the
 * normalizer-ready diagnostic rows (Lekalo rule ids, logical paths,
 * half-open ranges, namespaced original codes) plus one `unsupported`
 * row per rule whose prerequisite evidence was missing.
 *
 * @param array<int, array<string, mixed>> $diagnostics
 * @param array<int, array<string, mixed>> $symbols
 * @return array{findings: array<int, array<string, mixed>>, unsupported: array<int, string>}
 */
function strict_profile_evaluate(array $diagnostics, array $symbols): array
{
    $findings = [];
    $unsupported = [];
    foreach (STRICT_RULES as $id => $row) {
        if ($row['source'] === 'lint') {
            $matched = strict_map_lint_row($diagnostics, (string) $row['mago_code'], (string) $row['rule']);
            if ($matched === false) {
                $unsupported[] = (string) $id;
                continue;
            }
            foreach ($matched as $finding) {
                $findings[] = $finding;
            }
            continue;
        }
        $predicate = $id === 'final-readonly-profile'
            ? strict_predicate_final_readonly($symbols)
            : strict_predicate_unavailable((string) $id);
        if ($predicate === null) {
            $unsupported[] = (string) $id;
            continue;
        }
        foreach ($predicate as $finding) {
            $findings[] = $finding;
        }
    }
    return ['findings' => $findings, 'unsupported' => $unsupported];
}

/**
 * Map one Mago lint code onto its registered Lekalo row. `false` means
 * the prerequisite lint evidence was not in the receipt (the row's rule
 * never ran) — that is `unsupported`, not `compliant`.
 *
 * @param array<int, array<string, mixed>> $diagnostics
 * @return array<int, array<string, mixed>>|false
 */
function strict_map_lint_row(array $diagnostics, string $magoCode, string $ruleId): array|false
{
    $coverage = false;
    $findings = [];
    foreach ($diagnostics as $item) {
        if (($item['producer'] ?? '') !== 'mago') {
            continue;
        }
        if (($item['original_code'] ?? '') === $magoCode) {
            $coverage = true;
            $findings[] = $item;
        }
    }
    return $coverage ? $findings : false;
}

/**
 * The final/readonly profile predicate over symbol evidence: mutable
 * (non-final, non-abstract) domain classes are violations unless they
 * carry the explicit `extensible` marker in their modifiers. Symbol
 * evidence absent ⇒ `null` (unsupported), never a pass.
 *
 * @param array<int, array<string, mixed>> $symbols
 * @return array<int, array<string, mixed>>|null
 */
function strict_predicate_final_readonly(array $symbols): ?array
{
    $classes = array_values(array_filter(
        $symbols,
        static fn (array $symbol): bool => in_array($symbol['kind'], ['class', 'interface', 'trait', 'enum'], true),
    ));
    if ($classes === []) {
        return null;
    }
    $findings = [];
    foreach ($classes as $symbol) {
        $modifiers = $symbol['modifiers'] ?? [];
        $extensible = in_array('extensible', array_map('strval', $modifiers), true);
        if ($extensible || in_array('final', $modifiers, true) || in_array('abstract', $modifiers, true)) {
            continue;
        }
        $findings[] = [
            'rule' => 'target.analysis.final-readonly-profile',
            'original_code' => 'lekalo.final-profile',
            'level' => 'warning',
            'path' => $symbol['path'],
            'range' => $symbol['range'],
            'message' => 'domain class ' . (string) $symbol['identity'] . ' is neither final nor explicitly extensible',
            'producer' => 'lekalo',
        ];
    }
    return $findings;
}

/**
 * Rows whose prerequisite evidence does not exist in this slice: the
 * service-locator and magic-state predicates need resolved-reference
 * evidence the pinned toolchain does not export as a public graph.
 * Unsupported is the honest state (issue boundary: no regex fallback).
 */
function strict_predicate_unavailable(string $id): ?array
{
    return null;
}

/**
 * The single explicit `evidence-unsupported` diagnostic for one rule.
 *
 * @return array<string, mixed>
 */
function strict_unsupported_diagnostic(string $id): array
{
    return [
        'rule' => 'target.analysis.evidence-unsupported',
        'original_code' => 'lekalo.unsupported:' . $id,
        'level' => 'note',
        'path' => '.lekalo/import/mago/receipt.json',
        'range' => ['start' => 0, 'end' => 0],
        'message' => 'strict-profile rule ' . $id . ' lacks prerequisite evidence and is unsupported, not passing',
        'producer' => 'lekalo',
    ];
}
/**
 * The `lekalo.target/v1` protocol kernel of `lekalo-target-php-laravel`
 * (issue #54) — the PHP reference implementation of the target protocol.
 *
 * The kernel is a read-only-by-default PHP program built on PHP built-ins
 * only: no Composer package, no extension beyond `json` and `hash` (both
 * bundled and always compiled in). The protocol lifecycle mirrors the
 * Node kernel (issue #43) exactly: strict bounded request decode, closed
 * request validation, canonical response serialization, the mandatory
 * `describe` handshake, and honest `unsupported` refusals — never fake
 * successes.
 *
 * Runtime entry points:
 *   php adapter.php                                  one-shot protocol
 *                                                    process (stdin or
 *                                                    --lekalo-request-file)
 *   php adapter.php --version-json                   local metadata probe
 *                                                    (sole argument)
 *
 * Boundary decisions frozen for #54 (see README.md):
 * - The v0.2.16 wire has no runtime-version slot and no root-bearing
 *   profile transport; PHP runtime metadata is reported by the local
 *   `--version-json` probe and internal evidence only.
 * - Generation rides the closed write-plan seam: a dry run plans exact
 *   digests, an apply writes exactly those bytes inside the staged view.
 * - Unsupported operations are honest in-envelope `unsupported` errors,
 *   never silent lowering; core-side refused requests are bounded stderr
 *   diagnostics plus a nonzero exit, never a synthetic envelope.
 *
 * Issue #55 extends the kernel with the external-analyzer seam: the
 * kernel stays subprocess-free (it never launches Mago, Composer, or a
 * shell) and instead consumes a runner-produced analyzer receipt inside
 * its declared read view. Validate/verify keep their #54 empty-success
 * behavior when the analysis seam is absent; once a receipt exists, an
 * incompatible or failed analysis refuses semantic claims instead of
 * reporting a fake pass. Safe fixes are advice-only: no request can
 * ever apply one.
 */

// The protocol transport is exact-byte stdout: PHP CLI notice/warning
// rendering (which targets STDOUT by default) is disabled before any
// code can emit, and error logging is bound to the bounded stderr
// channel. A polluted envelope is a response refusal at the core, so
// the guard runs first, unconditionally.
ini_set('display_errors', '0');
ini_set('display_startup_errors', '0');
ini_set('log_errors', '1');
ini_set('error_log', 'php://stderr');
ini_set('implicit_flush', '0');
error_reporting(E_ALL);

const PROTOCOL_TOKEN = 'lekalo.target/v1';
/** The sole protocol version this kernel speaks (the current contract). */
const VERSION = '0.3.2';
/** The closed supported-version set: exact membership, never ranges. */
const SUPPORTED_VERSIONS = ['0.3.2','0.6.4'];
/** The adapter identity token. */
const ADAPTER_ID = 'lekalo-target-php-laravel';
/**
 * The adapter identity token.
 */
const ADAPTER_VERSION = '0.2.0';
/** The adapter target token (the wire `target` of generate/bind). */
const TARGET_TOKEN = 'php-laravel';
/** The default profile token; the strict profile rides the analysis seam. */
const PROFILE_TOKEN = 'default';
const STRICT_PROFILE_TOKEN = 'strict';
/** The accepted core IR contract version. */
const IR_VERSION = '0.2.16';

/** The maximum request size this kernel reads (mirrors the core bound). */
const MAX_REQUEST_BYTES = 1024 * 1024;
/** The maximum response size this kernel writes (mirrors the core cap). */
const MAX_RESPONSE_BYTES = 8 * 1024 * 1024;
/** The maximum single generated file size (mirrors the core artifact bound). */
const MAX_FILE_BYTES = 4 * 1024 * 1024;
/** The maximum number of files one generation plan may declare. */
const MAX_WRITE_FILES = 1024;
/** The closed v1 operation set (wire spellings). */
const OPERATIONS = [
    'describe', 'scan', 'bind', 'validate', 'generate',
    'verify', 'clean', 'plan-clean', 'plan-native', 'lint',
];
/** Operations that consume the compiled IR. */
const IR_OPERATIONS = ['validate', 'generate', 'verify'];
/** The closed support-state set (issue #28). */
const SUPPORT_STATES = ['full', 'partial', 'unsupported', 'unknown'];
/** The declared named capabilities of this kernel (issue #28 ids). */
const DECLARED_CAPABILITIES = [
    'lint.ai-readability' => 'partial',
    'generate.zod' => 'unsupported',
    'generate.openapi' => 'unsupported',
    'generate.ui' => 'unsupported',
    'generate.types' => 'full',
    'scan.symbols' => 'unsupported',
    'verify.scenarios' => 'full',
    'verify.transport-http' => 'unsupported',
    'generate.transport-http' => 'unsupported',
    'preserve.classification' => 'unsupported',
];

/**
 * The generated scenario-test write scope (issue #56).
 */
const SCENARIO_WRITE_SCOPES = ['src/generated/php-laravel/scenario-tests/**'];

/**
 * The user-owned scaffold scope (issue #56, plan S3): `scaffolded`
 * bindings emit once here and are never rewritten or deleted by the
 * kernel. Declared in `write_scopes` for apply honesty; the delete
 * path refuses it outright.
 */
const PHP_SCAFFOLD_SCOPE = 'tests/lekalo/scenario-tests/**';

/**
 * The observed scan index the checked-binding join reads.
 */
const PHP_OBSERVED_INDEX_PATH = '.lekalo/import/observed/index.json';

/**
 * The canonical evidence home of the compiled project IR (core-owned).
 */
const IR_EVIDENCE_HOME = '.lekalo/cache/ir';

/**
 * The scenario compiler modules, in fixed load order. `build.php`
 * concatenates the kernel plus these modules into the shipped
 * single-file artifact, so inside the shipped artifact the functions
 * are already defined and loading is a no-op; the source-tree kernel
 * loads them directly.
 */
function load_scenario_modules(): void
{
    static $loaded = false;
    if ($loaded) {
        return;
    }
    $loaded = true;
    if (function_exists('php_map_scenario') && function_exists('php_emit_scenario_tests')) {
        return;
    }
    foreach ([__DIR__ . '/scenario-map.php', __DIR__ . '/scenario-emit.php'] as $module) {
        if (!is_file($module)) {
            throw new RequestRefusal('compiler-module-missing');
        }
        require_once $module;
    }
}

/**
 * The native gate modules, in fixed load order (issue #61). Inside the
 * shipped artifact the functions are already defined and loading is a
 * no-op; the source-tree kernel loads them directly.
 */
function load_native_modules(): void
{
    static $loaded = false;
    if ($loaded) {
        return;
    }
    $loaded = true;
    if (function_exists('php_build_native_plan') && function_exists('php_verify_confirmations')) {
        return;
    }
    foreach ([__DIR__ . '/native-policy.php', __DIR__ . '/native-plan.php'] as $module) {
        if (!is_file($module)) {
            throw new RequestRefusal('native-module-missing');
        }
        require_once $module;
    }
}

/**
 * The type-generator modules, in fixed load order (issue #58). The
 * same lazy pattern as the scenario compiler: inside the shipped
 * artifact the modules are already appended and loading is a no-op;
 * the source-tree kernel loads them directly.
 */
function load_type_modules(): void
{
    static $loaded = false;
    if ($loaded) {
        return;
    }
    $loaded = true;
    if (function_exists('php_map_types') && function_exists('php_emit_types')) {
        return;
    }
    foreach ([
        __DIR__ . '/type-policy.php',
        __DIR__ . '/type-map.php',
        __DIR__ . '/type-codec.php',
        __DIR__ . '/type-emit.php',
        __DIR__ . '/type-bindings.php',
    ] as $module) {
        if (!is_file($module)) {
            throw new RequestRefusal('compiler-module-missing');
        }
        require_once $module;
    }
}

/**
 * The checked-in Composer execution policy bytes (issue #61). The
 * shipped artifact embeds them verbatim (build.php, custody binds to
 * the artifact digest); the source-tree kernel reads the checked-in
 * file beside the build script. Either way the bytes are the trusted
 * launch input — confirmation data, never execution authority.
 */
function native_gates_policy_document(): array
{
    if (defined('NATIVE_GATES_POLICY_BUNDLED')) {
        $bytes = NATIVE_GATES_POLICY_BUNDLED;
    } else {
        $path = dirname(__DIR__) . '/composer-gates-policy.json';
        $bytes = is_file($path) ? (string) file_get_contents($path) : '';
    }
    if ($bytes === '') {
        throw new RequestRefusal('native-policy-absent');
    }
    try {
        $document = json_decode($bytes, true, 64, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        throw new RequestRefusal('native-policy-unparsable');
    }
    if (!is_array($document)) {
        throw new RequestRefusal('native-policy-unparsable');
    }
    return $document;
}

/**
 * One request that failed closed decoding/validation before any
 * operation could run. Always maps to a bounded stderr diagnostic plus
 * exit 1 — a synthetic envelope with a fabricated echo is never legal.
 */
/**
 * The operations-generator modules, in fixed load order (issue #59).
 * They depend on the type modules' naming and codec helpers, so the
 * type modules load first. Inside the shipped artifact everything is
 * appended and loading is a no-op.
 */
function load_operation_modules(): void
{
    static $loaded = false;
    if ($loaded) {
        return;
    }
    $loaded = true;
    load_type_modules();
    if (function_exists('php_validate_operations_input') && function_exists('php_emit_operations')) {
        return;
    }
    foreach ([
        __DIR__ . '/operation-policy.php',
        __DIR__ . '/operation-map.php',
        __DIR__ . '/operation-emit.php',
        __DIR__ . '/operation-bindings.php',
    ] as $module) {
        if (!is_file($module)) {
            throw new RequestRefusal('compiler-module-missing');
        }
        require_once $module;
    }
}

/**
 * The routes-generator modules, in fixed load order (issue #60). They
 * depend on the type modules' naming and codec helpers and on the
 * operations modules' handler naming, so both load first. Inside the
 * shipped artifact everything is appended and loading is a no-op.
 */
function load_route_modules(): void
{
    static $loaded = false;
    if ($loaded) {
        return;
    }
    $loaded = true;
    load_operation_modules();
    if (function_exists('php_validate_routes_input') && function_exists('php_emit_routes')) {
        return;
    }
    foreach ([
        __DIR__ . '/route-policy.php',
        __DIR__ . '/route-map.php',
        __DIR__ . '/route-emit.php',
    ] as $module) {
        if (!is_file($module)) {
            throw new RequestRefusal('compiler-module-missing');
        }
        require_once $module;
    }
}

final class RequestRefusal extends RuntimeException
{
    public function __construct(string $code)
    {
        parent::__construct($code);
    }
}

// ---------------------------------------------------------------------------
// 1. Bounded input reading and strict JSON decoding.
// ---------------------------------------------------------------------------

/**
 * Read the request bytes from stdin or `--lekalo-request-file PATH`,
 * bounded at exactly MAX_REQUEST_BYTES; one more byte is a refusal.
 */
function read_request_bytes(): string
{
    $path = null;
    foreach ($_SERVER['argv'] ?? [] as $index => $argument) {
        if ($argument === '--lekalo-request-file') {
            if ($path !== null) {
                throw new RequestRefusal('transport');
            }
            $path = $_SERVER['argv'][$index + 1] ?? null;
            if ($path === null || $path === '') {
                throw new RequestRefusal('transport');
            }
        }
    }
    if ($path !== null) {
        $handle = @fopen($path, 'rb');
        if ($handle === false) {
            throw new RequestRefusal('transport');
        }
        $meta = stream_get_meta_data($handle);
        if (($meta['wrapper_type'] ?? '') !== 'plainfile') {
            fclose($handle);
            throw new RequestRefusal('transport');
        }
    } else {
        $handle = STDIN;
    }
    $chunks = [];
    $total = 0;
    while (!feof($handle)) {
        $chunk = fread($handle, 64 * 1024);
        if ($chunk === false) {
            if ($handle !== STDIN) {
                fclose($handle);
            }
            throw new RequestRefusal('transport');
        }
        $total += strlen($chunk);
        if ($total > MAX_REQUEST_BYTES) {
            if ($handle !== STDIN) {
                fclose($handle);
            }
            throw new RequestRefusal('request-too-large');
        }
        $chunks[] = $chunk;
    }
    if ($handle !== STDIN) {
        fclose($handle);
    }
    return implode('', $chunks);
}

/**
 * Decode exactly one JSON document with fatal malformed-UTF-8, closed
 * depth, and trailing-document refusals. `json_decode` collapses
 * duplicate keys, so a pre-pass scanner rejects duplicate decoded keys
 * (including `{"a":1,"\u0061":2}` alias collisions) before the value
 * decoder can silently last-wins them.
 */
function decode_json_document(string $bytes): array
{
    if (strlen($bytes) > MAX_REQUEST_BYTES) {
        throw new RequestRefusal('request-too-large');
    }
    if (!preg_match('//u', $bytes)) {
        throw new RequestRefusal('utf-8');
    }
    reject_duplicate_keys($bytes);
    $value = json_decode($bytes, true, 64, JSON_THROW_ON_ERROR);
    if (!is_json_object($value)) {
        throw new RequestRefusal('shape');
    }
    return $value;
}

final class DuplicateKeyScanner
{
    public int $position = 0;

    public function __construct(private readonly string $text)
    {
    }

    public function scan(): void
    {
        $this->skip();
        $this->value(0);
        $this->skip();
        if ($this->position !== strlen($this->text)) {
            throw new RequestRefusal('trailing-content');
        }
    }

    private function value(int $depth): void
    {
        if ($depth > 64) {
            throw new RequestRefusal('depth');
        }
        $char = $this->text[$this->position] ?? '';
        switch ($char) {
            case '{':
                $this->object($depth);
                return;
            case '[':
                $this->array($depth);
                return;
            case '"':
                $this->string();
                return;
            case '':
                throw new RequestRefusal('syntax');
            default:
                $this->literal();
        }
    }

    private function object(int $depth): void
    {
        $this->position++;
        $keys = [];
        $this->skip();
        if (($this->text[$this->position] ?? '') === '}') {
            $this->position++;
            return;
        }
        for (;;) {
            $this->skip();
            if (($this->text[$this->position] ?? '') !== '"') {
                throw new RequestRefusal('syntax');
            }
            $start = $this->position;
            $this->string();
            $raw = substr($this->text, $start, $this->position - $start);
            $key = json_decode($raw, true, 64, JSON_THROW_ON_ERROR);
            if (in_array($key, $keys, true)) {
                throw new RequestRefusal('duplicate-key');
            }
            $keys[] = $key;
            $this->skip();
            if (($this->text[$this->position] ?? '') !== ':') {
                throw new RequestRefusal('syntax');
            }
            $this->position++;
            $this->skip();
            $this->value($depth + 1);
            $this->skip();
            $char = $this->text[$this->position] ?? '';
            if ($char === ',') {
                $this->position++;
                continue;
            }
            if ($char === '}') {
                $this->position++;
                return;
            }
            throw new RequestRefusal('syntax');
        }
    }

    private function array(int $depth): void
    {
        $this->position++;
        $this->skip();
        if (($this->text[$this->position] ?? '') === ']') {
            $this->position++;
            return;
        }
        for (;;) {
            $this->skip();
            $this->value($depth + 1);
            $this->skip();
            $char = $this->text[$this->position] ?? '';
            if ($char === ',') {
                $this->position++;
                continue;
            }
            if ($char === ']') {
                $this->position++;
                return;
            }
            throw new RequestRefusal('syntax');
        }
    }

    private function string(): void
    {
        $this->position++;
        for (;;) {
            $char = $this->text[$this->position] ?? '';
            if ($char === '') {
                throw new RequestRefusal('syntax');
            }
            if ($char === '"') {
                $this->position++;
                return;
            }
            if ($char === '\\') {
                $escape = $this->text[$this->position + 1] ?? '';
                if ($escape === 'u') {
                    $hex = substr($this->text, $this->position + 2, 4);
                    if (!preg_match('/^[0-9a-fA-F]{4}$/', $hex)) {
                        throw new RequestRefusal('syntax');
                    }
                    $this->position += 6;
                    continue;
                }
                if (!in_array($escape, ['"', '\\', '/', 'b', 'f', 'n', 'r', 't'], true)) {
                    throw new RequestRefusal('syntax');
                }
                $this->position += 2;
                continue;
            }
            if (ord($char) < 0x20) {
                throw new RequestRefusal('syntax');
            }
            $this->position++;
        }
    }

    private function literal(): void
    {
        $rest = substr($this->text, $this->position);
        foreach (['true', 'false', 'null'] as $word) {
            if (str_starts_with($rest, $word)) {
                $this->position += strlen($word);
                return;
            }
        }
        if (preg_match('/^-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?/', $rest, $matches)) {
            $this->position += strlen($matches[0]);
            return;
        }
        throw new RequestRefusal('syntax');
    }

    private function skip(): void
    {
        while ($this->position < strlen($this->text)) {
            $char = $this->text[$this->position];
            if ($char === ' ' || $char === "\t" || $char === "\n" || $char === "\r") {
                $this->position++;
                continue;
            }
            break;
        }
    }
}

/**
 * Pre-pass duplicate-key rejection over the raw document: one scan that
 * mirrors the value walk exactly, recording every decoded object key.
 */
function reject_duplicate_keys(string $bytes): void
{
    (new DuplicateKeyScanner($bytes))->scan();
}

// ---------------------------------------------------------------------------
// 2. Canonical JSON output.
// ---------------------------------------------------------------------------

/**
 * Canonical compact JSON: keys in UTF-8 byte order, no whitespace, no
 * escaped slashes, exactly like the core serializer's output on closed
 * shapes. JSON numbers here are only integers and strings (the closed
 * response vocabulary never carries floats), so PHP/serde_json float
 * spellings never diverge.
 */
function canonical_json(array|bool|int|string|null $value): string
{
    $text = write_canonical($value);
    if (strlen($text) > MAX_RESPONSE_BYTES) {
        throw new RequestRefusal('response-too-large');
    }
    return $text;
}

function write_canonical(array|bool|int|string|null $value): string
{
    if ($value === null) {
        return 'null';
    }
    if (is_bool($value)) {
        return $value ? 'true' : 'false';
    }
    if (is_int($value)) {
        return (string) $value;
    }
    if (is_string($value)) {
        return canonical_string($value);
    }
    $isList = array_is_list($value);
    if ($isList) {
        return '[' . implode(',', array_map(__FUNCTION__, $value)) . ']';
    }
    $keys = array_keys($value);
    usort($keys, 'strcmp');
    $body = [];
    foreach ($keys as $key) {
        $body[] = canonical_string((string) $key)
            . ':' . write_canonical($value[$key]);
    }
    return '{' . implode(',', $body) . '}';
}

/**
 * Canonical JSON string encoding, byte-compatible with the core's
 * `serde_json` serializer: raw UTF-8 non-ASCII (never `\uXXXX`-escaped),
 * unescaped `/`, the closed escape set (`"`, `\`, and control
 * characters), and every other byte — DEL (U+007F) included — carried
 * raw. json_encode would silently DROP a DEL byte, so it travels as a
 * raw NUL sentinel through the encoder and is restored after; raw NUL
 * can never reach this function because the strict decoder refuses
 * control characters in request strings.
 */
function canonical_string(string $value): string
{
    $sentinel = str_replace("\x7F", "\x00", $value);
    $encoded = json_encode(
        $sentinel,
        JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR,
    );
    return str_replace('\\u0000', "\x7F", $encoded);
}

/** SHA-256 hex of the given UTF-8 bytes (binding anchors and digests). */
function sha256_hex(string $text): string
{
    return hash('sha256', $text);
}

/** The canonical `sha256:<64 hex>` digest spelling over bytes. */
function sha256_digest(string $bytes): string
{
    return 'sha256:' . hash('sha256', $bytes);
}

// ---------------------------------------------------------------------------
// 3. Grammar predicates (mirrors of the core scope module).
// ---------------------------------------------------------------------------

function is_sha256_digest(mixed $value): bool
{
    if (!is_string($value) || !str_starts_with($value, 'sha256:')) {
        return false;
    }
    $hex = substr($value, 7);
    return strlen($hex) === 64 && (bool) preg_match('/^[0-9a-f]{64}$/', $hex);
}

function is_request_id(mixed $value): bool
{
    $hex = is_string($value) ? substr($value, 4) : '';
    return is_string($value)
        && str_starts_with($value, 'req-')
        && strlen($hex) === 64
        && (bool) preg_match('/^[0-9a-f]{64}$/', $hex);
}

function is_plan_id(mixed $value): bool
{
    $hex = is_string($value) ? substr($value, 5) : '';
    return is_string($value)
        && str_starts_with($value, 'plan-')
        && strlen($hex) === 64
        && (bool) preg_match('/^[0-9a-f]{64}$/', $hex);
}

function is_contract_version(mixed $value): bool
{
    if (!is_string($value) || $value === '' || strlen($value) > 32) {
        return false;
    }
    $parts = explode('.', $value);
    if (count($parts) !== 3) {
        return false;
    }
    foreach ($parts as $part) {
        // Extension-free digit probe: strspn ships with ext/standard, so
        // the check survives `php -n` runtimes where ctype is absent.
        if ($part === '' || strspn($part, '0123456789') !== strlen($part)) {
            return false;
        }
    }
    return true;
}

/** One adapter id/target/profile token: lowercase first, closed charset. */
function is_token(mixed $value): bool
{
    return is_string($value)
        && $value !== ''
        && strlen($value) <= 64
        && (bool) preg_match('/^[a-z][a-z0-9-]*$/', $value);
}

/** One capability id: closed lowercase dotted-segment grammar. */
function is_capability_id(mixed $value): bool
{
    if (!is_string($value) || $value === '' || strlen($value) > 128) {
        return false;
    }
    foreach (explode('.', $value) as $segment) {
        if ($segment === '' || strlen($segment) > 64) {
            return false;
        }
        if (!preg_match('/^[a-z0-9][a-z0-9_-]*$/', $segment)) {
            return false;
        }
    }
    return true;
}

/**
 * One diagnostic rule id: the closed `target.analysis.*` family with
 * dotted segment grammar (the wire rule id, not the provider code).
 */
function is_analysis_rule_id(mixed $value): bool
{
    if (!is_string($value) || $value === '' || strlen($value) > 128
        || !str_starts_with($value, 'target.analysis.')) {
        return false;
    }
    foreach (explode('.', $value) as $segment) {
        if ($segment === '' || strlen($segment) > 64) {
            return false;
        }
        if (!preg_match('/^[a-z0-9][a-z0-9_-]*$/', $segment)) {
            return false;
        }
    }
    return true;
}

/**
 * One symbol identity: a bounded lowercase dotted-segment grammar
 * (`php.fixture.demo` is the convention; segments may include `_` and
 * `-`). The `php.` prefix is convention, not grammar — the prefix is
 * enforced by the closed segment charset, not by a reserved token.
 */
function is_symbol_identity(mixed $value): bool
{
    if (!is_string($value) || $value === '' || strlen($value) > 191) {
        return false;
    }
    foreach (explode('.', $value) as $segment) {
        if ($segment === '' || strlen($segment) > 64) {
            return false;
        }
        if (!preg_match('/^[a-z0-9][a-z0-9_-]*$/', $segment)) {
            return false;
        }
    }
    return true;
}

/**
 * One native evidence path: case-preserving, traversal-free, rooted
 * (no leading separator), with bounded segments. This is the versioned
 * native-path domain for target-project sources (see the diagnostic
 * decoder note on Laravel casing); it is deliberately distinct from the
 * lowercase Model logical-path grammar, and protected homes are still
 * refused by the same rule the wire enforces.
 */
function is_native_evidence_path(mixed $value): bool
{
    if (!is_string($value) || $value === '' || strlen($value) > 512 || str_starts_with($value, '/')) {
        return false;
    }
    $parts = explode('/', $value);
    if (in_array('', $parts, true)) {
        return false;
    }
    foreach ($parts as $part) {
        if ($part === '.' || $part === '..' || strlen($part) > 255) {
            return false;
        }
        if ((bool) preg_match('/[\x00-\x1f]/', $part)) {
            return false;
        }
        if ((bool) preg_match('/^[A-Za-z]:/', $part) || str_contains($part, '\\')) {
            return false;
        }
    }
    return protected_home($value) === null;
}

function segment_ok(string $segment): bool
{
    if ($segment === '.' || $segment === '..' || str_ends_with($segment, '.')) {
        return false;
    }
    if ($segment === '' || strlen($segment) > 64) {
        return false;
    }
    if (!preg_match('/^[a-z0-9.]([a-z0-9._-]*)$/', $segment)) {
        return false;
    }
    if (!preg_match('/^[a-z0-9.]/', $segment)) {
        return false;
    }
    return true;
}

/** Whether one path is a grammatical logical path (no `**`). */
function is_logical_path(mixed $value): bool
{
    if (!is_string($value) || $value === '' || strlen($value) > 512 || str_starts_with($value, '/')) {
        return false;
    }
    $parts = explode('/', $value);
    if (in_array('', $parts, true)) {
        return false;
    }
    foreach ($parts as $part) {
        if (!segment_ok($part)) {
            return false;
        }
    }
    return true;
}

/** Whether one value is a grammatical scope: optional trailing `**`. */
function is_scope(mixed $value): bool
{
    if (!is_string($value) || $value === '' || strlen($value) > 512 || str_starts_with($value, '/')) {
        return false;
    }
    $parts = explode('/', $value);
    if (in_array('', $parts, true)) {
        return false;
    }
    $last = array_pop($parts);
    foreach ($parts as $part) {
        if (!segment_ok($part)) {
            return false;
        }
    }
    if ($last === '**') {
        return $parts !== [];
    }
    return segment_ok($last);
}

function scope_covers(string $scope, string $path): bool
{
    if (!is_scope($scope) || !is_logical_path($path)) {
        return false;
    }
    $scopeParts = explode('/', $scope);
    $pathParts = explode('/', $path);
    $recursive = $scopeParts[array_key_last($scopeParts)] === '**';
    $prefix = $recursive ? array_slice($scopeParts, 0, -1) : $scopeParts;
    if ($recursive) {
        return count($pathParts) > count($prefix)
            && array_slice($pathParts, 0, count($prefix)) === $prefix;
    }
    return $pathParts === $prefix;
}

/** The protected canonical homes an adapter may never write (core mirror). */
function protected_home(string $path): ?string
{
    $parts = explode('/', $path);
    $homes = [
        'lekalo-model' => ['lekalo'],
        'lekalo-lockfile' => ['lekalo.lock'],
        'ir' => ['.lekalo', 'ir'],
        'cache' => ['.lekalo', 'cache'],
        'import' => ['.lekalo', 'import'],
        'privacy' => ['.lekalo', 'privacy'],
        'consumer' => ['.lekalo', 'consumer'],
        'openspec' => ['openspec'],
    ];
    foreach ($homes as $name => $home) {
        if (count($parts) >= count($home)
            && array_slice($parts, 0, count($home)) === $home) {
            return $name;
        }
    }
    return null;
}

// ---------------------------------------------------------------------------
// 4. Closed request validation (every core `validate_request` rule).
// ---------------------------------------------------------------------------

/**
 * The closed request envelope members; the decoder rejects unknown
 * members and explicit nulls before this point.
 */
function validate_request_object(array $document): array
{
    static $keys = [
        'protocol', 'protocol_version', 'operation', 'request_id',
        'project_root', 'ir_path', 'target', 'profile', 'profile_digest',
        'profile_capabilities', 'dry_run', 'limits', 'plan_id',
        'native_request',
        'lint_request',
    ];
    foreach (array_keys($document) as $key) {
        if (!in_array($key, $keys, true)) {
            throw new RequestRefusal('unknown-key');
        }
        if ($document[$key] === null) {
            throw new RequestRefusal('null-member');
        }
    }
    foreach (['protocol', 'protocol_version', 'operation', 'request_id', 'project_root'] as $key) {
        if (!array_key_exists($key, $document)) {
            throw new RequestRefusal('missing-key');
        }
    }
    if ($document['protocol'] !== PROTOCOL_TOKEN) {
        throw new RequestRefusal('protocol-token');
    }
    if (!in_array($document['protocol_version'], SUPPORTED_VERSIONS, true)) {
        throw new RequestRefusal('protocol-version');
    }
    if (!is_string($document['operation']) || !in_array($document['operation'], OPERATIONS, true)) {
        throw new RequestRefusal('operation');
    }
    if (!is_request_id($document['request_id'])) {
        throw new RequestRefusal('request-id');
    }
    if ($document['project_root'] !== '.') {
        throw new RequestRefusal('project-root');
    }
    foreach ([['ir_path', 'is_logical_path'], ['target', 'is_token'], ['profile', 'is_token']] as [$key, $check]) {
        if (array_key_exists($key, $document) && !$check($document[$key])) {
            throw new RequestRefusal('grammar');
        }
    }
    $operation = $document['operation'];
    if (array_key_exists('limits', $document)) {
        $limits = $document['limits'];
        if (!is_array($limits) || !is_json_object($limits)) {
            throw new RequestRefusal('shape');
        }
        $limitKeys = array_keys($limits);
        sort($limitKeys, SORT_STRING);
        if ($limitKeys !== ['max_output_bytes', 'timeout_ms']) {
            throw new RequestRefusal('shape');
        }
        $timeout = $limits['timeout_ms'];
        $output = $limits['max_output_bytes'];
        if (!is_int($timeout) || $timeout < 1 || $timeout > 3600000
            || !is_int($output) || $output < 1 || $output > 1073741824) {
            throw new RequestRefusal('shape');
        }
    }
    $hasPlanId = array_key_exists('plan_id', $document);
    $apply = $operation === 'clean'
        || ($operation === 'generate' && ($document['dry_run'] ?? null) === false);
    if ($apply !== $hasPlanId || ($hasPlanId && !is_plan_id($document['plan_id']))) {
        throw new RequestRefusal('plan-id');
    }
    $hasNative = array_key_exists('native_request', $document);
    if (($operation === 'plan-native') !== $hasNative) {
        throw new RequestRefusal('native-request');
    }
    if ($operation === 'plan-native') {
        if (array_key_exists('dry_run', $document) || $hasPlanId) {
            throw new RequestRefusal('plan-id');
        }
        if (!in_array($document['protocol_version'],SUPPORTED_VERSIONS,true)) {
            throw new RequestRefusal('member');
        }
        validate_native_request($document['native_request']);
    }
    if (($operation === 'generate') !== array_key_exists('dry_run', $document)) {
        throw new RequestRefusal('dry-run');
    }
    if (array_key_exists('dry_run', $document) && !is_bool($document['dry_run'])) {
        throw new RequestRefusal('dry-run');
    }
    $requiresIr = in_array($operation, IR_OPERATIONS, true);
    if ($requiresIr !== array_key_exists('ir_path', $document)) {
        throw new RequestRefusal('ir-path');
    }
    if (in_array($operation, ['generate', 'bind'], true) && !array_key_exists('target', $document)) {
        throw new RequestRefusal('target');
    }
    if ($operation === 'bind' && !array_key_exists('profile', $document)) {
        throw new RequestRefusal('profile');
    }
    if ($operation === 'describe') {
        foreach (['target', 'profile', 'profile_digest', 'profile_capabilities', 'ir_path'] as $key) {
            if (array_key_exists($key, $document)) {
                throw new RequestRefusal('member');
            }
        }
    }
    $hasDigest = array_key_exists('profile_digest', $document);
    $hasCapabilities = array_key_exists('profile_capabilities', $document);
    if ($hasDigest !== $hasCapabilities) {
        throw new RequestRefusal('profile-capabilities');
    }
    if ($hasDigest) {
        if (!in_array($document['protocol_version'], SUPPORTED_VERSIONS, true)) {
            throw new RequestRefusal('member');
        }
        if (!array_key_exists('profile', $document)) {
            throw new RequestRefusal('profile');
        }
        if (!is_sha256_digest($document['profile_digest'])) {
            throw new RequestRefusal('profile-digest');
        }
        validate_profile_capabilities($document['profile_capabilities']);
    }
    if (in_array($operation, ['generate', 'bind'], true)
        && !is_token($document['target'] ?? null)) {
        throw new RequestRefusal('grammar');
    }
    if (!function_exists('lint_validate_request')) {require_once __DIR__ . '/ai-lint.php';}
    lint_validate_request($document);
    return $document;
}

function validate_profile_capabilities(mixed $capabilities): void
{
    if (!is_array($capabilities) || !array_is_list($capabilities)
        || count($capabilities) === 0 || count($capabilities) > 64) {
        throw new RequestRefusal('profile-capabilities');
    }
    $previous = '';
    foreach ($capabilities as $capability) {
        // Member closure is order-insensitive (serde `deny_unknown_fields`
        // never depends on decoded member order), so exactly the closed
        // pair — in any key order — is legal.
        if (!is_json_object($capability)
            || count($capability) !== 2
            || !array_key_exists('id', $capability)
            || !array_key_exists('support', $capability)) {
            throw new RequestRefusal('profile-capabilities');
        }
        if (!is_capability_id($capability['id'])
            || !in_array($capability['support'], SUPPORT_STATES, true)) {
            throw new RequestRefusal('profile-capabilities');
        }
        if (strcmp($capability['id'], $previous) <= 0) {
            throw new RequestRefusal('profile-capabilities');
        }
        $previous = $capability['id'];
    }
}

/**
 * Validate the closed `native_request` member (issue #48): bounded
 * changed inputs plus digest-addressed custody references — never a
 * command or an absolute URL.
 */
/**
 * Whether one decoded array can only be a JSON object: a non-empty list
 * is definitely a JSON array, but an empty PHP array is ambiguous — JSON
 * `{}` and `[]` both decode to `[]`, so an empty array is accepted as
 * the (member-less) object shape.
 */
function is_json_object(mixed $value): bool
{
    return is_array($value) && (!array_is_list($value) || $value === []);
}

function validate_native_request(mixed $native): void
{
    static $keys = [
        'changes', 'scan_ref', 'observed_ref', 'execution_policy_ref',
        'input_manifest_digest', 'tool_catalog_digest',
        'capability_snapshot_digest',
    ];
    if (!is_json_object($native)) {
        throw new RequestRefusal('native-request');
    }
    foreach (array_keys($native) as $key) {
        if (!in_array($key, $keys, true)) {
            throw new RequestRefusal('native-request');
        }
    }
    foreach (['changes', 'scan_ref', 'execution_policy_ref', 'input_manifest_digest',
        'tool_catalog_digest', 'capability_snapshot_digest'] as $key) {
        if (!array_key_exists($key, $native)) {
            throw new RequestRefusal('native-request');
        }
    }
    $changes = $native['changes'];
    if (!is_json_object($changes)) {
        throw new RequestRefusal('native-request');
    }
    $files = $changes['files'] ?? [];
    if (!is_array($files) || !array_is_list($files) || count($files) > 1024) {
        throw new RequestRefusal('native-request');
    }
    foreach ($files as $file) {
        if (!is_json_object($file)) {
            throw new RequestRefusal('native-request');
        }
        foreach (array_keys($file) as $key) {
            if (!in_array($key, ['path', 'change', 'before_digest', 'after_digest'], true)) {
                throw new RequestRefusal('native-request');
            }
        }
        if (!isset($file['path']) || !is_logical_path($file['path'])) {
            throw new RequestRefusal('native-request');
        }
        if (!isset($file['change'])
            || !in_array($file['change'], ['added', 'modified', 'deleted', 'renamed'], true)) {
            throw new RequestRefusal('native-request');
        }
        foreach (['before_digest', 'after_digest'] as $key) {
            if (array_key_exists($key, $file) && !is_sha256_digest($file[$key])) {
                throw new RequestRefusal('native-request');
            }
        }
    }
    $symbols = $changes['symbols'] ?? [];
    if (!is_array($symbols) || !array_is_list($symbols) || count($symbols) > 1024) {
        throw new RequestRefusal('native-request');
    }
    validate_native_content_ref($native['scan_ref']);
    if (array_key_exists('observed_ref', $native)) {
        validate_native_content_ref($native['observed_ref']);
    }
    validate_native_content_ref($native['execution_policy_ref']);
    foreach (['input_manifest_digest', 'tool_catalog_digest', 'capability_snapshot_digest'] as $key) {
        if (!is_sha256_digest($native[$key])) {
            throw new RequestRefusal('native-request');
        }
    }
}

function validate_native_content_ref(mixed $reference): void
{
    if (!is_json_object($reference)
        || !is_sha256_digest($reference['digest'] ?? null)) {
        throw new RequestRefusal('native-request');
    }
    foreach (array_keys($reference) as $key) {
        if (!in_array($key, ['digest', 'revision', 'adapter'], true)) {
            throw new RequestRefusal('native-request');
        }
    }
    if (array_key_exists('revision', $reference)) {
        $revision = $reference['revision'];
        if (!is_string($revision) || $revision === '' || strlen($revision) > 128) {
            throw new RequestRefusal('native-request');
        }
    }
    if (array_key_exists('adapter', $reference) && !is_token($reference['adapter'])) {
        throw new RequestRefusal('native-request');
    }
}

// ---------------------------------------------------------------------------
// 5. The deterministic generation seam (issue #54 MVP scope).
// ---------------------------------------------------------------------------

/**
 * The generated-artifact entry the kernel owns: one TypeScript-free PHP
 * side artifact per operation, proving the closed dry-run/apply/clean
 * write-plan seam over the fixture IR. The MVP generation surface is
 * deliberately minimal and honest: the adapter declares no deep
 * generator capabilities (those are the #55/#56 seams), and generation
 * never claims a construct it did not map.
 */
/**
 * Whether the request's `ir_path` names a scenario document: either the
 * closed `.scenario.json` spelling or the conformance fixture's exact
 * scenario input path.
 */
function is_scenario_ir_path(string $path): bool
{
    return str_ends_with($path, '.scenario.json')
        || str_ends_with($path, 'scenario-txn-concurrency.json')
        || str_contains(basename($path), '.scenario.');
}

/**
 * The bounded types-input path convention (issue #58): documents under
 * `lekalo/types/` carrying the `.types.json` suffix drive the type
 * generator, exactly like the scenario and migration conventions.
 */
function is_types_ir_path(string $path): bool
{
    return str_starts_with($path, 'lekalo/types/')
        && str_ends_with($path, '.types.json');
}

/**
 * The deterministic generation entry (issue #56): a scenario document
 * at `ir_path` maps to the full Laratesto test set (support files plus
 * one test and one canonical sidecar per scenario); anything else
 * falls back to the #54 kernel artifact. A generation over a scenario
 * document carries the mapper's typed findings; a compile-time finding
 * vetoes every write exactly like the Node pipeline.
 */
function deterministic_generation(array $request): array
{
    $irPath = $request['ir_path'] ?? '';
    if (is_string($irPath) && is_scenario_ir_path($irPath)) {
        return scenario_deterministic_generation($request, $irPath);
    }
    $routesRequest = resolve_routes_request($request);
    if ($routesRequest !== null) {
        return routes_deterministic_generation($routesRequest);
    }
    $operationsRequest = resolve_operations_request($request);
    if ($operationsRequest !== null) {
        return operations_deterministic_generation($operationsRequest);
    }
    $typesRequest = resolve_types_request($request);
    if ($typesRequest !== null) {
        return types_deterministic_generation($typesRequest);
    }
    return kernel_deterministic_generation($request);
}

/** Whether one path is the operations input home. */
function is_operations_ir_path(string $path): bool
{
    return str_starts_with($path, 'lekalo/operations/')
        && str_ends_with($path, '.operations.json');
}

/** Whether one path is the routes input home (issue #60). */
function is_routes_ir_path(string $path): bool
{
    return str_starts_with($path, 'lekalo/routes/')
        && str_ends_with($path, '.routes.json');
}

/**
 * The routes request for one incoming request, or null when the request
 * does not drive routes generation: either the ir_path is already a
 * routes input document, or it is the core's staged IR evidence whose
 * project carries a declared routes input beside it (issue #60). The
 * routing must stay a pure read decision.
 */
function resolve_routes_request(array $request): ?array
{
    $irPath = $request['ir_path'] ?? '';
    if (!is_string($irPath)) {
        return null;
    }
    if (is_routes_ir_path($irPath)) {
        return $request;
    }
    if (!is_types_evidence_path($irPath)) {
        return null;
    }
    $routesDoc = 'lekalo/routes/' . basename($irPath, '.json') . '.routes.json';
    if (!is_routes_ir_path($routesDoc) || read_view_file($routesDoc) === null) {
        return null;
    }
    $flipped = $request;
    $flipped['ir_path'] = $routesDoc;
    return $flipped;
}

/**
 * The operations request for one incoming request, or null when the
 * request does not drive operations generation: either the ir_path is
 * already an operations input document, or it is the core's staged IR
 * evidence whose project carries a declared operations input beside it
 * (issue #59). The operations family requires the types input too; an
 * operations run without the bound types document refuses at generation
 * time, not here (the routing must stay a pure read decision).
 */
function resolve_operations_request(array $request): ?array
{
    $irPath = $request['ir_path'] ?? '';
    if (!is_string($irPath)) {
        return null;
    }
    if (is_operations_ir_path($irPath)) {
        return $request;
    }
    if (!is_types_evidence_path($irPath)) {
        return null;
    }
    $operationsDoc = 'lekalo/operations/' . basename($irPath, '.json') . '.operations.json';
    if (!is_operations_ir_path($operationsDoc) || read_view_file($operationsDoc) === null) {
        return null;
    }
    $flipped = $request;
    $flipped['ir_path'] = $operationsDoc;
    return $flipped;
}

/** The scenario branch of the deterministic generation entry. */
function scenario_deterministic_generation(array $request, string $irPath): array
{
    {
        $outcome = scenario_generation($request);
        if (isset($outcome['refusal'])) {
            throw new RequestRefusal($outcome['refusal']);
        }
        if ($outcome['findings'] !== []) {
            // Capability honesty: the mapper cannot express the document.
            // Nothing is emitted and nothing is written.
            return [
                'path' => null,
                'bytes' => '',
                'digest' => null,
                'writes' => [],
                'findings' => $outcome['findings'],
            ];
        }
        $writes = [];
        $skipWrites = $outcome['skip_writes'] ?? [];
        foreach ($outcome['files'] as $file) {
            if (isset($skipWrites[$file['path']])) {
                continue;
            }
            $writes[] = [
                'path' => $file['path'],
                'action' => 'create',
                'sha256' => $file['digest'],
            ];
        }
        return [
            'path' => null,
            'bytes' => '',
            'digest' => null,
            'writes' => $writes,
            'files' => $outcome['files'],
            'findings' => [],
        ];
    }
}

/**
 * The types request for one incoming request, or null when the request
 * does not drive type generation: either the ir_path is already a
 * types input document, or it is the core's staged IR evidence whose
 * project carries a declared types input beside it (issue #58).
 */
function resolve_types_request(array $request): ?array
{
    $irPath = $request['ir_path'] ?? '';
    if (!is_string($irPath)) {
        return null;
    }
    if (is_types_ir_path($irPath)) {
        return $request;
    }
    if (!is_types_evidence_path($irPath)) {
        return null;
    }
    $typesDoc = 'lekalo/types/' . basename($irPath, '.json') . '.types.json';
    if (!is_types_ir_path($typesDoc) || read_view_file($typesDoc) === null) {
        return null;
    }
    $flipped = $request;
    $flipped['ir_path'] = $typesDoc;
    return $flipped;
}

/** Whether one path is the core's staged IR evidence home. */
function is_types_evidence_path(string $path): bool
{
    return str_starts_with($path, IR_EVIDENCE_HOME . '/')
        && str_ends_with($path, '.json');
}

/** The types branch of the deterministic generation entry. */
function types_deterministic_generation(array $request): array
{
    {
        $outcome = type_generation($request);
        if (isset($outcome['refusal'])) {
            throw new RequestRefusal($outcome['refusal']);
        }
        if ($outcome['findings'] !== []) {
            // Capability honesty: an unsupported projection or a failed
            // checked join vetoes every write — zero partial emission,
            // zero success receipt.
            return [
                'path' => null,
                'bytes' => '',
                'digest' => null,
                'writes' => [],
                'findings' => $outcome['findings'],
            ];
        }
        $writes = [];
        $skipWrites = $outcome['skip_writes'] ?? [];
        foreach ($outcome['files'] as $file) {
            if (isset($skipWrites[$file['path']])) {
                continue;
            }
            $writes[] = [
                'path' => $file['path'],
                'action' => 'create',
                'sha256' => $file['digest'],
            ];
        }
        return [
            'path' => null,
            'bytes' => '',
            'digest' => null,
            'writes' => $writes,
            'files' => $outcome['files'],
            'findings' => [],
        ];
    }
}

/**
 * The composed types + operations branch of the deterministic
 * generation entry (issue #59): missing managed types and operations
 * generate in ONE authorized plan. Overlapping paths or any failed
 * required family vetoes every write before publication.
 */
function operations_deterministic_generation(array $request): array
{
    $outcome = operations_generation($request);
    if (isset($outcome['refusal'])) {
        throw new RequestRefusal($outcome['refusal']);
    }
    if ($outcome['findings'] !== []) {
        // Capability honesty: any compile-time finding vetoes every
        // write across BOTH families - zero partial publication.
        return [
            'path' => null,
            'bytes' => '',
            'digest' => null,
            'writes' => [],
            'findings' => $outcome['findings'],
        ];
    }
    $writes = [];
    $skipWrites = $outcome['skip_writes'] ?? [];
    $files = array_map(
        static fn (array $file): array => [
            'path' => $file['path'],
            'bytes' => $file['text'],
            'digest' => $file['digest'],
            'frozen' => $file['lifecycle'] === 'scaffolded',
            'marker' => $file['lifecycle'] === 'scaffolded'
                ? PHP_OPERATIONS_SCAFFOLD_ROOT . '/operations.map.json'
                : null,
        ],
        $outcome['files'],
    );
    foreach ($outcome['files'] as $file) {
        if (isset($skipWrites[$file['path']])) {
            continue;
        }
        $writes[] = [
            'path' => $file['path'],
            'action' => 'create',
            'sha256' => $file['digest'],
        ];
    }
    return [
        'path' => null,
        'bytes' => '',
        'digest' => null,
        'writes' => $writes,
        'files' => $files,
        'findings' => [],
    ];
}

/**
 * The read-and-map-and-emit flow of the composed operations run: the
 * validated operations input names the exact IR evidence and the bound
 * #58 types input; both families plan together; managed emissions land
 * under the generated root, scaffold-once emissions once under the
 * closed consumer root, checked and custom records never write.
 */
function operations_generation(array $request): array
{
    load_operation_modules();
    $irPath = (string) ($request['ir_path'] ?? '');
    $inputText = read_view_file($irPath);
    if ($inputText === null) {
        return ['refusal' => 'operations-input-unreadable'];
    }
    try {
        $document = json_decode($inputText, true, 512, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return ['refusal' => 'operations-input-shape'];
    }
    if (is_array($document)
        && isset($document['schemaVersion'], $document['identity'])
        && ($document['schemaVersion'] !== PHP_OPERATIONS_INPUT_SCHEMA_VERSION
            || $document['identity'] !== PHP_OPERATIONS_INPUT_IDENTITY)) {
        return ['refusal' => 'operations-input-identity'];
    }
    $input = php_validate_operations_input($document);
    if ($input === null) {
        return ['refusal' => 'operations-input-shape'];
    }
    $findingOnly = static fn (array $findings): array => [
        'files' => [],
        'findings' => $findings,
        'skip_writes' => [],
    ];
    // The bound types family: required dependency, exact digest.
    $typesDoc = 'lekalo/types/' . $input['projectId'] . '.types.json';
    $typesText = read_view_file($typesDoc);
    if ($typesText === null || !is_types_ir_path($typesDoc)) {
        return $findingOnly([[
            'path' => $input['projectId'] . '.operations',
            'code' => 'operations.types-unbound',
            'detail' => 'the operations input requires the bound types input document',
        ]]);
    }
    $typesDigest = sha256_digest($typesText);
    if ($typesDigest !== $input['typesInputDigest']) {
        return $findingOnly([[
            'path' => $input['projectId'] . '.operations',
            'code' => 'operations.types-unbound',
            'detail' => 'the input names different types-input bytes than the committed document',
        ]]);
    }
    $typesRequest = $request;
    $typesRequest['ir_path'] = $typesDoc;
    $typesOutcome = types_deterministic_generation($typesRequest);
    if (isset($typesOutcome['refusal'])) {
        return ['refusal' => $typesOutcome['refusal']];
    }
    // Required-family findings veto the composed run: the types family
    // (checked custody, unsupported projections) answers a findings-only
    // envelope with no files, and publishing operations alone would be
    // exactly the partial publication the composition refuses. The rows
    // are already wire findings; they merge into the composed veto.
    if (($typesOutcome['findings'] ?? []) !== []) {
        return $findingOnly($typesOutcome['findings']);
    }
    // The compiled IR evidence: the only bytes an adapter may read.
    $irEvidenceText = read_view_file(IR_EVIDENCE_HOME . '/' . $input['projectId'] . '.json');
    if ($irEvidenceText === null) {
        return ['refusal' => 'operations-ir-unreadable'];
    }
    try {
        $irEvidence = json_decode($irEvidenceText, true, 512, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return ['refusal' => 'operations-ir-shape'];
    }
    if (!is_array($irEvidence) || ($irEvidence['contract'] ?? null) !== PHP_TYPES_IR_IDENTITY) {
        return ['refusal' => 'operations-ir-identity'];
    }
    if (sha256_digest($irEvidenceText) !== $input['irDigest']) {
        return ['refusal' => 'operations-input-digest'];
    }
    // The mapped type inventory (the naming authority the handlers reuse).
    try {
        $typesInput = php_validate_types_input(json_decode($typesText, true, 512, JSON_THROW_ON_ERROR));
    } catch (JsonException) {
        return ['refusal' => 'operations-input-shape'];
    }
    if ($typesInput === null) {
        return ['refusal' => 'operations-types-unbound'];
    }
    $mappedTypes = php_map_types([
        'ir' => $irEvidence,
        'policy' => $typesInput['policy'],
        'irDigest' => $typesInput['irDigest'],
        'inputDigest' => $typesDigest,
    ]);
    if ($mappedTypes['state'] !== 'mapped') {
        return $findingOnly(types_wire_findings($mappedTypes['findings']));
    }
    $definitions = [];
    foreach ($irEvidence['definitions'] ?? [] as $index => $definition) {
        if (is_array($definition) && is_string($definition['id'] ?? null)) {
            $definitions[$definition['id']] = $definition;
        }
    }
    $context = [
        'input' => $input,
        'definitions' => $definitions,
        'typesIndex' => $mappedTypes['index'],
        'collections' => $mappedTypes['collections'],
        'namespacePrefix' => $input['namespacePrefix'],
        'root' => PHP_OPERATIONS_GENERATED_ROOT,
        'irDigest' => $input['irDigest'],
        'inputDigest' => sha256_digest($inputText),
        'typesInputDigest' => $typesDigest,
    ];
    $emitContext = [
        'projectId' => $input['projectId'],
        'context' => $context,
        'definitions' => $definitions,
        'typesIndex' => $mappedTypes['index'],
        'namespacePrefix' => $input['namespacePrefix'],
    ];
    $files = [];
    $findings = [];
    $skipWrites = [];
    $claimed = [];
    foreach (($typesOutcome['files'] ?? []) as $file) {
        $claimed[$file['path']] = true;
        // Normalize the types rows onto the operations row shape.
        $files[] = [
            'path' => $file['path'],
            'text' => $file['bytes'],
            'digest' => $file['digest'],
            'role' => 'types',
            'lifecycle' => 'generated',
        ];
    }
    foreach (['emittable' => PHP_OPERATIONS_GENERATED_ROOT, 'scaffold' => PHP_OPERATIONS_SCAFFOLD_ROOT] as $root) {
        $records = array_values(array_filter(
            $input['operations'],
            static fn (array $record): bool => $root === PHP_OPERATIONS_SCAFFOLD_ROOT
                ? $record['mode'] === 'scaffold-once'
                : in_array($record['mode'], ['managed'], true),
        ));
        $checkedRecords = $root === PHP_OPERATIONS_GENERATED_ROOT
            ? array_values(array_filter(
                $input['operations'],
                static fn (array $record): bool => in_array($record['mode'], ['checked', 'custom'], true),
            ))
            : [];
        $runContext = $context;
        $runContext['root'] = $root;
        $runContext['input'] = array_merge($input, ['operations' => $records]);
        if ($records !== []) {
            $mapped = php_map_operations($runContext);
            if ($mapped['state'] !== 'mapped') {
                return $findingOnly(types_wire_findings($mapped['findings']));
            }
            // Scaffold custody: with the marker present the whole root
            // is user-owned and nothing is planned; with the marker
            // absent, ANY pre-existing path refuses.
            $marker = $root . '/operations.map.json';
            $emitted = php_emit_operations(array_merge($emitContext, [
                'mapped' => $mapped,
                'root' => $root,
            ]));
            if ($root === PHP_OPERATIONS_SCAFFOLD_ROOT) {
                if (is_file($marker)) {
                    foreach ($emitted['files'] as $file) {
                        $skipWrites[$file['path']] = true;
                    }
                    foreach ($emitted['files'] as $file) {
                        if (!isset($claimed[$file['path']])) {
                            $claimed[$file['path']] = true;
                            $files[] = $file;
                        }
                    }
                } else {
                    foreach ($emitted['files'] as $file) {
                        if (is_file($file['path'])) {
                            return ['refusal' => 'operations-scaffold-unowned'];
                        }
                    }
                    foreach ($emitted['files'] as $file) {
                        if (!isset($claimed[$file['path']])) {
                            $claimed[$file['path']] = true;
                            $files[] = $file;
                        }
                    }
                }
            } else {
                foreach ($emitted['files'] as $file) {
                    if (isset($claimed[$file['path']])) {
                        throw new RequestRefusal('operations-path-collision');
                    }
                    $claimed[$file['path']] = true;
                    $files[] = $file;
                }
            }
        }
        if ($checkedRecords !== []) {
            $evidence = read_operations_evidence();
            foreach (php_check_operation_bindings($checkedRecords, $definitions, $evidence, $input, $context['inputDigest'], static function (string $path): ?string {
                $digest = is_file($path) ? hash_file('sha256', $path) : false;
                return $digest === false ? null : 'sha256:' . $digest;
            }) as $bindingFinding) {
                $findings[] = types_wire_findings([$bindingFinding])[0];
            }
        }
    }
    usort($files, static fn (array $left, array $right): int => strcmp((string) $left['path'], (string) $right['path']));
    return [
        'files' => $files,
        'findings' => $findings,
        'skip_writes' => $skipWrites,
    ];
}

/** The parsed observed operations evidence, or null when absent/corrupt. */
function read_operations_evidence(): ?array
{
    $text = read_view_file(PHP_OPERATIONS_EVIDENCE_PATH);
    if ($text === null) {
        return null;
    }
    try {
        $document = json_decode($text, true, 512, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return null;
    }
    return php_validate_operations_evidence($document);
}

/**
 * The operations validate exchange (issue #59): the same composed
 * read-and-map path generate uses, but no writes ever result.
 */
function operations_validate_response(array $request): array
{
    $outcome = operations_generation($request);
    if (isset($outcome['refusal'])) {
        return ['error' => [
            'class' => 'invalid',
            'code' => $outcome['refusal'],
            'message' => 'the operations input could not be read or mapped',
            'retryable' => false,
            'partial' => false,
        ]];
    }
    return build_response($request, ['result' => ['ok' => true, 'findings' => $outcome['findings']]]);
}

/**
 * The operations verify exchange (issue #59): managed files are
 * compared by exact digest (missing/drifted are operations.drift);
 * scaffold files are existence-checked under the surviving marker;
 * checked and custom records re-run the strict shape join.
 */
function operations_verify_response(array $request): array
{
    $outcome = operations_generation($request);
    if (isset($outcome['refusal'])) {
        return ['error' => [
            'class' => 'invalid',
            'code' => $outcome['refusal'],
            'message' => 'the operations input could not be read or mapped',
            'retryable' => false,
            'partial' => false,
        ]];
    }
    $findings = $outcome['findings'];
    foreach ($outcome['files'] as $file) {
        if (str_starts_with((string) $file['path'], PHP_OPERATIONS_SCAFFOLD_ROOT . '/')) {
            // Scaffold-once custody: user-owned, existence-checked only
            // under the surviving marker. A skipped regeneration (the
            // marker-guarded emission of this same run) is the only row
            // that may report a removed scaffold: with no marker the
            // emission refusal already named the unowned path.
            $marker = PHP_OPERATIONS_SCAFFOLD_ROOT . '/operations.map.json';
            $markerGuarded = isset($outcome['skip_writes'][$file['path']]);
            if ($markerGuarded && !is_file($file['path']) && is_file($marker)) {
                $findings[] = [
                    'path' => $file['path'],
                    'code' => 'operations.scaffold-missing',
                    'detail' => 'scaffolded-operation-removed',
                ];
            }
            continue;
        }
        $expectedDigest = $file['digest'];
        $actual = is_file($file['path']) ? hash_file('sha256', $file['path']) : false;
        if ($actual === false) {
            $findings[] = ['path' => $file['path'], 'code' => 'operations.drift', 'detail' => 'missing'];
        } elseif ($actual !== substr((string) $expectedDigest, 7)) {
            $findings[] = ['path' => $file['path'], 'code' => 'operations.drift', 'detail' => 'drifted'];
        }
    }
    return build_response($request, ['result' => ['ok' => true, 'findings' => $findings]]);
}

/**
 * The composed types + operations + routes branch of the deterministic
 * generation entry (issue #60): the routes family runs ON TOP of the
 * composed #59 run — a route wrapper without its handler would be dead
 * code — and every family finding vetoes the whole plan before any
 * publication.
 */
function routes_deterministic_generation(array $request): array
{
    $outcome = routes_generation($request);
    if (isset($outcome['refusal'])) {
        throw new RequestRefusal($outcome['refusal']);
    }
    if ($outcome['findings'] !== []) {
        // Capability honesty: any compile-time finding vetoes every
        // write across ALL families - zero partial publication.
        return [
            'path' => null,
            'bytes' => '',
            'digest' => null,
            'writes' => [],
            'findings' => $outcome['findings'],
        ];
    }
    $writes = [];
    $skipWrites = $outcome['skip_writes'] ?? [];
    $files = array_map(
        static fn (array $file): array => [
            'path' => $file['path'],
            'bytes' => $file['text'],
            'digest' => $file['digest'],
            'frozen' => false,
            'marker' => null,
        ],
        $outcome['files'],
    );
    foreach ($outcome['files'] as $file) {
        if (isset($skipWrites[$file['path']])) {
            continue;
        }
        $writes[] = [
            'path' => $file['path'],
            'action' => 'create',
            'sha256' => $file['digest'],
        ];
    }
    return [
        'path' => null,
        'bytes' => '',
        'digest' => null,
        'writes' => $writes,
        'files' => $files,
        'findings' => [],
    ];
}

/**
 * The read-and-map-and-emit flow of the composed routes run (issue
 * #60): the validated routes input pins the exact IR evidence, the
 * canonical transport attachment, the bound types and operations
 * inputs; the managed routes land under the generated routes root and
 * the checked records join the observed routes evidence.
 */
function routes_generation(array $request): array
{
    load_route_modules();
    $irPath = (string) ($request['ir_path'] ?? '');
    $inputText = read_view_file($irPath);
    if ($inputText === null) {
        return ['refusal' => 'routes-input-unreadable'];
    }
    try {
        $document = json_decode($inputText, true, 512, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return ['refusal' => 'routes-input-shape'];
    }
    if (is_array($document)
        && isset($document['schemaVersion'], $document['identity'])
        && ($document['schemaVersion'] !== PHP_ROUTES_INPUT_SCHEMA_VERSION
            || $document['identity'] !== PHP_ROUTES_INPUT_IDENTITY)) {
        return ['refusal' => 'routes-input-identity'];
    }
    $input = php_validate_routes_input($document);
    if ($input === null) {
        return ['refusal' => 'routes-input-shape'];
    }
    $findingOnly = static fn (array $findings): array => [
        'files' => [],
        'findings' => $findings,
        'skip_writes' => [],
    ];
    // The staged evidence: the only bytes an adapter may read.
    $irEvidenceText = read_view_file(IR_EVIDENCE_HOME . '/' . $input['projectId'] . '.json');
    if ($irEvidenceText === null) {
        return ['refusal' => 'routes-ir-unreadable'];
    }
    try {
        $irEvidence = json_decode($irEvidenceText, true, 512, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return ['refusal' => 'routes-ir-shape'];
    }
    if (!is_array($irEvidence) || ($irEvidence['contract'] ?? null) !== PHP_TYPES_IR_IDENTITY) {
        return ['refusal' => 'routes-ir-identity'];
    }
    if (sha256_digest($irEvidenceText) !== $input['irDigest']) {
        return ['refusal' => 'routes-input-digest'];
    }
    // The canonical transport evidence: the wire authority the routes
    // project from. The digest joins the input pin.
    $transportText = read_view_file('.lekalo/cache/transport/' . $input['projectId'] . '.json');
    if ($transportText === null) {
        return ['refusal' => 'routes-transport-unreadable'];
    }
    try {
        $transport = json_decode($transportText, true, 512, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return ['refusal' => 'routes-transport-shape'];
    }
    if (!is_array($transport)
        || ($transport['schemaVersion'] ?? null) !== PHP_ROUTES_TRANSPORT_SCHEMA_VERSION
        || ($transport['identity'] ?? null) !== PHP_ROUTES_TRANSPORT_IDENTITY) {
        return ['refusal' => 'routes-transport-identity'];
    }
    $transportDigest = sha256_digest($transportText);
    if ($transportDigest !== $input['transportDigest']) {
        return ['refusal' => 'routes-transport-digest'];
    }
    // The staged OpenAPI projection: the same join renders the published
    // document and the boundary tables; the mapper joins its digests.
    $openapiText = read_view_file('.lekalo/cache/openapi/' . $input['projectId'] . '.json');
    $openapiDigest = $openapiText === null ? null : sha256_digest($openapiText);
    $openapi = null;
    if ($openapiText !== null) {
        try {
            $openapi = json_decode($openapiText, true, 512, JSON_THROW_ON_ERROR);
        } catch (JsonException) {
            $openapi = null;
        }
    }
    // The composed types + operations run: a route wrapper without its
    // handler join is dead code, and any family finding vetoes the
    // whole plan.
    $operationsDoc = 'lekalo/operations/' . $input['projectId'] . '.operations.json';
    $operationsText = read_view_file($operationsDoc);
    if ($operationsText === null || !is_operations_ir_path($operationsDoc)) {
        return $findingOnly([[
            'path' => $input['projectId'] . '.routes',
            'code' => 'routes.operations-unbound',
            'detail' => 'the routes input requires the bound operations input document',
        ]]);
    }
    if (sha256_digest($operationsText) !== $input['operationsInputDigest']) {
        return $findingOnly([[
            'path' => $input['projectId'] . '.routes',
            'code' => 'routes.operations-unbound',
            'detail' => 'the input names different operations-input bytes than the committed document',
        ]]);
    }
    $operationsRequest = $request;
    $operationsRequest['ir_path'] = $operationsDoc;
    $operationsOutcome = operations_generation($operationsRequest);
    if (isset($operationsOutcome['refusal'])) {
        return ['refusal' => $operationsOutcome['refusal']];
    }
    if (($operationsOutcome['findings'] ?? []) !== []) {
        return $findingOnly($operationsOutcome['findings']);
    }
    // The mapped type inventory (the naming authority the request
    // bindings reuse) and the operations input inventory (the entry
    // naming authority the wrappers invoke).
    try {
        $typesInput = php_validate_types_input(json_decode(
            (string) read_view_file('lekalo/types/' . $input['projectId'] . '.types.json'),
            true,
            512,
            JSON_THROW_ON_ERROR,
        ));
    } catch (JsonException) {
        return ['refusal' => 'routes-input-shape'];
    }
    if ($typesInput === null) {
        return $findingOnly([[
            'path' => $input['projectId'] . '.routes',
            'code' => 'routes.types-unbound',
            'detail' => 'the routes input requires the bound types input document',
        ]]);
    }
    $typesDocText = (string) read_view_file('lekalo/types/' . $input['projectId'] . '.types.json');
    $mappedTypes = php_map_types([
        'ir' => $irEvidence,
        'policy' => $typesInput['policy'],
        'irDigest' => $typesInput['irDigest'],
        'inputDigest' => sha256_digest($typesDocText),
    ]);
    if ($mappedTypes['state'] !== 'mapped') {
        return $findingOnly(types_wire_findings($mappedTypes['findings']));
    }
    try {
        $operationsInput = php_validate_operations_input(json_decode($operationsText, true, 512, JSON_THROW_ON_ERROR));
    } catch (JsonException) {
        return ['refusal' => 'routes-input-shape'];
    }
    if ($operationsInput === null) {
        return $findingOnly([[
            'path' => $input['projectId'] . '.routes',
            'code' => 'routes.operations-unbound',
            'detail' => 'the bound operations input document is not the accepted contract',
        ]]);
    }
    $definitions = [];
    foreach ($irEvidence['definitions'] ?? [] as $definition) {
        if (is_array($definition) && is_string($definition['id'] ?? null)) {
            $definitions[$definition['id']] = $definition;
        }
    }
    $context = [
        'input' => $input,
        'definitions' => $definitions,
        'typesIndex' => $mappedTypes['index'],
        'operationsInput' => $operationsInput,
        'transport' => $transport,
        'transportDigest' => $transportDigest,
        'openapi' => $openapi,
        'irDigest' => $input['irDigest'],
        'inputDigest' => sha256_digest($inputText),
        'typesInputDigest' => $input['typesInputDigest'],
        'operationsInputDigest' => $input['operationsInputDigest'],
        'openapiDigest' => $openapiDigest,
        'namespacePrefix' => $input['namespacePrefix'],
        'operationsNamespacePrefix' => (string) ($operationsInput['namespacePrefix'] ?? PHP_OPERATIONS_DEFAULT_NAMESPACE_PREFIX),
    ];
    $mapped = php_map_routes($context);
    if ($mapped['state'] !== 'mapped') {
        return $findingOnly(types_wire_findings($mapped['findings']));
    }
    $files = [];
    $findings = [];
    $skipWrites = [];
    $claimed = [];
    foreach (($operationsOutcome['files'] ?? []) as $file) {
        $claimed[$file['path']] = true;
        $files[] = [
            'path' => $file['path'],
            'text' => $file['text'],
            'digest' => $file['digest'],
            'role' => $file['role'] ?? 'operations',
            'lifecycle' => $file['lifecycle'] ?? 'generated',
        ];
    }
    // The managed routes emission under the closed generated root.
    $emitted = php_emit_routes([
        'projectId' => $input['projectId'],
        'mapped' => $mapped,
        'context' => $context,
        'openapiBytes' => $openapiText,
        'root' => PHP_ROUTES_GENERATED_ROOT,
    ]);
    foreach ($emitted['files'] as $file) {
        if (isset($claimed[$file['path']])) {
            throw new RequestRefusal('routes-path-collision');
        }
        $claimed[$file['path']] = true;
        $files[] = $file;
    }
    // The checked records: no writes; the declared surface joins the
    // observed routes evidence.
    $checkedRecords = array_values(array_filter(
        $input['routes'],
        static fn (array $record): bool => $record['mode'] === 'checked',
    ));
    if ($checkedRecords !== []) {
        $evidence = php_read_routes_evidence();
        foreach (php_routes_check_bindings(
            $checkedRecords,
            $mapped['routes'],
            $definitions,
            $evidence,
            $input,
            $context['inputDigest'],
            static function (string $path): ?string {
                $digest = is_file($path) ? hash_file('sha256', $path) : false;
                return $digest === false ? null : 'sha256:' . $digest;
            },
        ) as $bindingFinding) {
            $findings[] = types_wire_findings([$bindingFinding])[0];
        }
    }
    usort($files, static fn (array $left, array $right): int => strcmp((string) $left['path'], (string) $right['path']));
    return [
        'files' => $files,
        'findings' => $findings,
        'skip_writes' => $skipWrites,
    ];
}

/**
 * The routes validate exchange (issue #60): the same composed
 * read-and-map path generate uses, but no writes ever result.
 */
function routes_validate_response(array $request): array
{
    $outcome = routes_generation($request);
    if (isset($outcome['refusal'])) {
        return ['error' => [
            'class' => 'invalid',
            'code' => $outcome['refusal'],
            'message' => 'the routes input could not be read or mapped',
            'retryable' => false,
            'partial' => false,
        ]];
    }
    return build_response($request, ['result' => ['ok' => true, 'findings' => $outcome['findings']]]);
}

/**
 * The routes verify exchange (issue #60): managed files are compared by
 * exact digest (missing/drifted are routes.drift); checked records
 * re-run the strict evidence join.
 */
function routes_verify_response(array $request): array
{
    $outcome = routes_generation($request);
    if (isset($outcome['refusal'])) {
        return ['error' => [
            'class' => 'invalid',
            'code' => $outcome['refusal'],
            'message' => 'the routes input could not be read or mapped',
            'retryable' => false,
            'partial' => false,
        ]];
    }
    $findings = $outcome['findings'];
    foreach ($outcome['files'] as $file) {
        if (!str_starts_with((string) $file['path'], PHP_ROUTES_GENERATED_ROOT . '/')) {
            continue;
        }
        $expectedDigest = $file['digest'];
        $actual = is_file($file['path']) ? hash_file('sha256', $file['path']) : false;
        if ($actual === false) {
            $findings[] = ['path' => $file['path'], 'code' => 'routes.drift', 'detail' => 'missing'];
        } elseif ($actual !== substr((string) $expectedDigest, 7)) {
            $findings[] = ['path' => $file['path'], 'code' => 'routes.drift', 'detail' => 'drifted'];
        }
    }
    return build_response($request, ['result' => ['ok' => true, 'findings' => $findings]]);
}

/** The kernel-artifact fallback of the deterministic generation entry. */
function kernel_deterministic_generation(array $request): array
{
    $artifact = kernel_artifact($request);
    $writes = [[
        'path' => $artifact['path'],
        'action' => $artifact['action'] ?? 'create',
        'sha256' => $artifact['digest'],
    ]];
    $envelope = [
        'path' => $artifact['path'],
        'bytes' => $artifact['bytes'],
        'digest' => $artifact['digest'],
        'writes' => $writes,
        'findings' => [],
    ];
    if (isset($artifact['ledger'])) {
        // The migration emitter's append-only ledger rides beside the
        // migration file through the envelope: the write planner adds
        // its entry and the apply loop verifies its exact bytes.
        $envelope['ledger'] = $artifact['ledger'];
    }
    return $envelope;
}

/**
 * The scenario read-and-map path shared by generate/validate/verify:
 * reads the scenario document, the compiled project IR evidence from
 * the canonical cache home, and the project test-port declaration;
 * maps through the pure mapper; and returns the emitted files plus
 * typed findings. A read refusal or a closed-shape refusal is an
 * in-envelope `failed` outcome, never a guessed plan.
 */
function read_and_map_scenario(array $request): array
{
    $outcome = scenario_generation($request);
    if (isset($outcome['refusal'])) {
        return ['error' => [
            'class' => 'invalid',
            'code' => $outcome['refusal'],
            'message' => 'the scenario document could not be read or mapped',
            'retryable' => false,
            'partial' => false,
        ]];
    }
    return $outcome;
}

/**
 * The scenario compiler: map the scenario document and emit the
 * deterministic test files. Compilation only reads data: application,
 * vendor, and port code never execute inside the compiler process.
 */
function scenario_generation(array $request): array
{
    $irPath = $request['ir_path'] ?? '';
    if (!is_string($irPath) || !is_scenario_ir_path($irPath)) {
        // Not a scenario document: the #54 kernel artifact path owns it.
        return ['not-scenario' => true, 'files' => [], 'findings' => []];
    }
    $scenarioText = read_view_file($irPath);
    if ($scenarioText === null) {
        return ['refusal' => 'scenario-unreadable', 'files' => [], 'findings' => []];
    }
    try {
        $scenario = json_decode($scenarioText, true, 512, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return ['refusal' => 'scenario-shape', 'files' => [], 'findings' => []];
    }
    if (($scenario['schemaVersion'] ?? null) !== 'lekalo/scenario-ir/v0.2.16') {
        return ['refusal' => 'ir-version-unsupported', 'files' => [], 'findings' => []];
    }
    $projectId = $scenario['projectId'] ?? null;
    // The project id interpolates into the emitted PHP namespace
    // (Lekalo\Generated\ScenarioTests\{$projectId}): the grammar is
    // the core's PHP-identifier spelling — underscores, never hyphens —
    // so a hyphen-carrying id cannot emit an unparseable namespace.
    if (!is_string($projectId) || preg_match('/^[a-z][a-z0-9_]*$/', $projectId) !== 1) {
        return ['refusal' => 'scenario-project-id', 'files' => [], 'findings' => []];
    }
    $irEvidenceText = read_view_file(IR_EVIDENCE_HOME . '/' . $projectId . '.json');
    if ($irEvidenceText === null) {
        return ['refusal' => 'ir-evidence-unreadable', 'files' => [], 'findings' => []];
    }
    try {
        $irEvidence = json_decode($irEvidenceText, true, 512, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return ['refusal' => 'ir-evidence-shape', 'files' => [], 'findings' => []];
    }
    $port = null;
    $portPresent = false;
    load_scenario_modules();
    $portText = read_view_file(PHP_PORT_DOC_PATH);
    if ($portText !== null) {
        $portPresent = true;
        try {
            $port = json_decode($portText, true, 512, JSON_THROW_ON_ERROR);
        } catch (JsonException) {
            return ['refusal' => 'port-shape', 'files' => [], 'findings' => []];
        }
        if (!is_array($port)) {
            return ['refusal' => 'port-shape', 'files' => [], 'findings' => []];
        }
        // The adapter-owned declaration must satisfy its closed shape
        // before any plan exists: a present-but-invalid document is an
        // authoring error, never an all-unsupported silent fallback.
        $port = php_validate_port_doc($port);
        if ($port === null) {
            return ['refusal' => 'port-shape', 'files' => [], 'findings' => []];
        }
    }
    $capabilities = null;
    if (isset($request['profile_capabilities']) && is_array($request['profile_capabilities'])) {
        $capabilities = $request['profile_capabilities'];
    }
    $mapped = php_map_scenario([
        'scenario' => $scenario,
        'ir' => $irEvidence,
        'irDigest' => 'sha256:' . hash('sha256', $irEvidenceText),
        'port' => $port,
        'portPresent' => $portPresent,
        'profileCapabilities' => $capabilities,
    ]);
    if ($mapped['state'] === 'refused') {
        return ['refusal' => 'scenario-' . $mapped['refusal'], 'files' => [], 'findings' => []];
    }
    $files = php_emit_scenario_tests([
        'models' => $mapped['scenarios'],
        'inputDigest' => 'sha256:' . hash('sha256', $scenarioText),
        'adapterVersion' => ADAPTER_VERSION,
        'portModulePath' => $portPresent ? $port['path'] : '',
        'portClass' => $portPresent ? $port['class'] : '',
    ]);
    $emitted = [];
    $skipWrites = [];
    foreach ($files as $file) {
        $entry = [
            'path' => $file['path'],
            'bytes' => $file['text'],
            'digest' => 'sha256:' . hash('sha256', $file['text']),
        ];
        if (($file['frozen'] ?? false) === true) {
            // Scaffold-once custody: the frozen write is planned only
            // while its managed marker is absent. Once the marker
            // exists the file is user-owned — regeneration never
            // rewrites it, and a deleted test is a verify finding
            // rather than a silent recreate.
            $entry['frozen'] = true;
            $entry['marker'] = $file['marker'];
            if (is_file($file['marker'])) {
                $skipWrites[$file['path']] = true;
            }
        }
        $emitted[] = $entry;
    }
    return [
        'files' => $emitted,
        'findings' => $mapped['findings'],
        'skip_writes' => $skipWrites,
        'scenario' => $scenario,
    ];
}

/** One bounded read inside the declared read roots (null when absent). */
function read_view_file(string $path): ?string
{
    if (!is_logical_path($path) || strlen($path) > 4096) {
        return null;
    }
    if (str_starts_with($path, '.lekalo/ir/') || str_starts_with($path, '.lekalo/cache/')
        || str_starts_with($path, '.lekalo/import/')
        || str_starts_with($path, 'lekalo/')) {
        $bytes = @file_get_contents($path);
        return $bytes === false ? null : $bytes;
    }
    return null;
}

// ---------------------------------------------------------------------------
// The type generator (issue #58): map → emit → custody.
// ---------------------------------------------------------------------------

/**
 * The type generator entry: a types-input document at `ir_path` maps
 * the compiled IR to the closed type inventory and emits the custody's
 * files. A mapping finding or a failed checked join vetoes every write
 * exactly like the Node pipeline; the three custody modes select the
 * emission root and the custody flags, never a different compiler.
 */
function type_generation(array $request): array
{
    $irPath = $request['ir_path'] ?? '';
    if (!is_string($irPath) || !is_types_ir_path($irPath)) {
        // Not a types document: the scenario/migration/kernel paths own it.
        return ['not-types' => true, 'files' => [], 'findings' => []];
    }
    load_type_modules();
    $inputText = read_view_file($irPath);
    if ($inputText === null) {
        return ['refusal' => 'types-input-unreadable', 'files' => [], 'findings' => []];
    }
    try {
        $document = json_decode($inputText, true, 512, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return ['refusal' => 'types-input-shape', 'files' => [], 'findings' => []];
    }
    if (is_array($document)
        && isset($document['schemaVersion'], $document['identity'])
        && ($document['schemaVersion'] !== PHP_TYPES_INPUT_SCHEMA_VERSION
            || $document['identity'] !== PHP_TYPES_INPUT_IDENTITY)) {
        // A recognizable but different contract generation is an
        // identity refusal, never a shape guess.
        return ['refusal' => 'types-input-identity', 'files' => [], 'findings' => []];
    }
    $input = php_validate_types_input($document);
    if ($input === null) {
        return ['refusal' => 'types-input-shape', 'files' => [], 'findings' => []];
    }
    $irEvidenceText = read_view_file(IR_EVIDENCE_HOME . '/' . $input['projectId'] . '.json');
    if ($irEvidenceText === null) {
        return ['refusal' => 'types-ir-unreadable', 'files' => [], 'findings' => []];
    }
    try {
        $irEvidence = json_decode($irEvidenceText, true, 512, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return ['refusal' => 'types-ir-shape', 'files' => [], 'findings' => []];
    }
    if (!is_array($irEvidence) || ($irEvidence['contract'] ?? null) !== PHP_TYPES_IR_IDENTITY) {
        return ['refusal' => 'types-ir-identity', 'files' => [], 'findings' => []];
    }
    if (!is_sha256_digest($input['irDigest']) || sha256_digest($irEvidenceText) !== $input['irDigest']) {
        // The input names the exact evidence bytes it consumes: any
        // divergence is a binding refusal, never a best-effort read.
        return ['refusal' => 'types-input-digest', 'files' => [], 'findings' => []];
    }
    $policy = $input['policy'];
    $mapped = php_map_types([
        'ir' => $irEvidence,
        'policy' => $policy,
        'irDigest' => $input['irDigest'],
        'inputDigest' => sha256_digest($inputText),
    ]);
    if ($mapped['state'] === 'refused') {
        return ['refusal' => $mapped['refusal'], 'files' => [], 'findings' => []];
    }
    if ($policy['custody'] === 'checked') {
        // Checked custody: zero writes, read-only conformance against
        // the observed evidence. Missing evidence is a finding for
        // every declared id — never conformant, never a rewrite.
        $findings = php_check_type_bindings($mapped, read_types_evidence(),
            static function (string $path): ?string {
                $digest = is_file($path) ? hash_file('sha256', $path) : false;
                return $digest === false ? null : 'sha256:' . $digest;
            });
        return ['files' => [], 'findings' => types_wire_findings($findings), 'types' => $mapped];
    }
    if ($mapped['state'] === 'unsupported') {
        // Unsupported projections veto the whole generation: no partial
        // DTO publication, no success receipt, no fabricated capability.
        return ['files' => [], 'findings' => types_wire_findings($mapped['findings']), 'types' => $mapped];
    }
    $root = $policy['custody'] === 'scaffold-once' ? (string) $policy['scaffoldRoot'] : PHP_TYPES_GENERATED_ROOT;
    $emitted = php_emit_types([
        'projectId' => $input['projectId'],
        'mapped' => $mapped,
        'adapterVersion' => ADAPTER_VERSION,
        'root' => $root,
    ]);
    $files = [];
    $skipWrites = [];
    $scaffold = $policy['custody'] === 'scaffold-once';
    if ($scaffold) {
        // Scaffold-once custody: the sidecar is the bundle marker. With
        // the marker present the whole scaffold is user-owned and no
        // write is planned; with the marker absent, ANY pre-existing
        // path refuses — a marker alone never grants overwrite
        // authority, and an unowned file is never adopted.
        $marker = $root . '/types.map.json';
        if (is_file($marker)) {
            foreach ($emitted['files'] as $file) {
                $skipWrites[$file['path']] = true;
            }
        } else {
            foreach ($emitted['files'] as $file) {
                if (is_file($file['path'])) {
                    return ['refusal' => 'types-scaffold-unowned', 'files' => [], 'findings' => []];
                }
            }
        }
    }
    foreach ($emitted['files'] as $file) {
        $entry = [
            'path' => $file['path'],
            'bytes' => $file['text'],
            'digest' => 'sha256:' . hash('sha256', $file['text']),
        ];
        if ($scaffold) {
            $entry['frozen'] = true;
            $entry['marker'] = $root . '/types.map.json';
        }
        $files[] = $entry;
    }
    return ['files' => $files, 'findings' => [], 'skip_writes' => $skipWrites];
}

/** The parsed observed evidence document, or null when absent/corrupt. */
function read_types_evidence(): ?array
{
    $text = read_view_file(PHP_TYPES_EVIDENCE_PATH);
    if ($text === null) {
        return null;
    }
    try {
        $document = json_decode($text, true, 512, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return null;
    }
    return php_validate_types_evidence($document);
}

/**
 * Map the closed internal findings to the bounded wire rows: semantic
 * id as path, reason+pointer+detail in one clamped detail.
 */
function types_wire_findings(array $findings): array
{
    $rows = [];
    foreach ($findings as $finding) {
        $detail = ($finding['reason'] ?? $finding['code'])
            . (isset($finding['pointer']) ? ' at ' . $finding['pointer'] : '')
            . ': ' . ($finding['detail'] ?? '');
        $rows[] = [
            'path' => $finding['semanticId'] ?? 'php-types',
            'code' => $finding['code'],
            'detail' => utf8_safe_clamp($detail, 128),
        ];
    }
    return $rows;
}

/**
 * The types validate exchange (issue #58): the same read-and-map path
 * generate uses, but no writes ever result. Mapping findings are the
 * answer; an empty set is an honest empty findings answer.
 */
function types_validate_response(array $request): array
{
    $outcome = type_generation($request);
    if (isset($outcome['refusal'])) {
        return ['error' => [
            'class' => 'invalid',
            'code' => $outcome['refusal'],
            'message' => 'the types input could not be read or mapped',
            'retryable' => false,
            'partial' => false,
        ]];
    }
    return build_response($request, ['result' => ['ok' => true, 'findings' => $outcome['findings']]]);
}

/**
 * The types verify exchange (issue #58): managed files are compared by
 * exact digest (missing/drifted are `php-types.drift` findings); the
 * scaffold is existence-checked only (a removed file with a surviving
 * marker is `php-types.scaffold-missing`); checked custody re-runs the
 * strict join.
 */
function types_verify_response(array $request): array
{
    $outcome = type_generation($request);
    if (isset($outcome['refusal'])) {
        return ['error' => [
            'class' => 'invalid',
            'code' => $outcome['refusal'],
            'message' => 'the types input could not be read or mapped',
            'retryable' => false,
            'partial' => false,
        ]];
    }
    $findings = $outcome['findings'];
    foreach ($outcome['files'] as $file) {
        if (($file['frozen'] ?? false) === true) {
            if (!is_file($file['path']) && is_file($file['marker'])) {
                $findings[] = [
                    'path' => $file['path'],
                    'code' => 'php-types.scaffold-missing',
                    'detail' => 'scaffolded-type-removed',
                ];
            }
            continue;
        }
        $expectedDigest = $file['digest'];
        $actual = is_file($file['path']) ? hash_file('sha256', $file['path']) : false;
        if ($actual === false) {
            $findings[] = ['path' => $file['path'], 'code' => 'php-types.drift', 'detail' => 'missing'];
        } elseif ($actual !== substr($expectedDigest, 7)) {
            $findings[] = ['path' => $file['path'], 'code' => 'php-types.drift', 'detail' => 'drifted'];
        }
    }
    return build_response($request, ['result' => ['ok' => true, 'findings' => $findings]]);
}

/**
 * The closed read seam of the Composer planner (issue #61): the only
 * project bytes the planner may read are the root composer.json, the
 * root composer.lock, and the host-validated selection document under
 * the import home — each bounded, each inside the manifest's declared
 * read scopes. Anything else is invisible to planning.
 */
const NATIVE_READ_FILES = ['composer.json', 'composer.lock'];
const NATIVE_SELECTION_PATH = '.lekalo/import/native-selection.json';
/** The maximum composer.json bytes the planner reads (contract bound). */
const NATIVE_MAX_MANIFEST_BYTES = 1024 * 1024;
/** The maximum composer.lock bytes the planner digests (never parsed). */
const NATIVE_MAX_LOCK_BYTES = 8 * 1024 * 1024;

function native_read_project_bytes(string $path): ?string
{
    $limit = match ($path) {
        'composer.json' => NATIVE_MAX_MANIFEST_BYTES,
        'composer.lock' => NATIVE_MAX_LOCK_BYTES,
        default => null,
    }; 
    if ($limit === null) {
        return $path === NATIVE_SELECTION_PATH ? read_view_file($path) : null;
    }
    if (!is_file($path)) {
        return null;
    }
    $size = filesize($path);
    if ($size === false || $size > $limit) {
        return null;
    }
    $bytes = file_get_contents($path, false, null, 0, $limit + 1);
    if ($bytes === false || strlen($bytes) > $limit) {
        return null;
    }
    return $bytes;
}

/**
 * The generated-artifact entry the kernel owns when no scenario
 * document is present (the #54 MVP surface, kept for the conformance
 * battery). Generation never claims a construct it did not map.
 */
function kernel_artifact(array $request): array
{
    $target = $request['target'] ?? TARGET_TOKEN;
    $profile = $request['profile'] ?? PROFILE_TOKEN;
    $irPath = $request['ir_path'] ?? '.lekalo/ir/planner.json';
    // Issue #57: a bound Laravel migration input document drives the
    // migration emitter; everything else rides the kernel fixture
    // artifact. The input path must sit inside the declared read
    // scope and carry the closed contract identity.
    if (str_ends_with($irPath, '.migration-input.json')
        && scope_covers('.lekalo/ir/**', $irPath)
        && is_file($irPath)) {
        $bytes = file_get_contents($irPath);
        if ($bytes !== false) {
            $input = decode_storage_input($bytes);
            return emit_laravel_migrations($input, $target, $irPath);
        }
    }
    $header = sprintf(
        "<?php\n\n// generated by %s@%s\ndeclare(strict_types=1);\n\n// target: %s\n// profile: %s\n// ir: %s\n",
        ADAPTER_ID,
        ADAPTER_VERSION,
        $target,
        $profile,
        $irPath,
    );
    $body = "return [\n    'kernel' => '%s',\n];\n";
    $text = $header . sprintf($body, ADAPTER_ID);
    $path = sprintf('.lekalo/generated/php-laravel/%s/kernel.php', $target);
    return [
        'path' => $path,
        'bytes' => $text,
        'digest' => sha256_digest($text),
    ];
}

/** The write entries of one generation outcome (already computed). */
function deterministic_writes(array $artifact): array
{
    $writes = $artifact['writes'];
    // The migration emitter ships its append-only ledger beside the
    // migration file; both must be planned and written atomically. A
    // first publish plans a create, an append plans a replace of the
    // merged ledger (the append itself is proven by
    // assert_append_only inside the emitter).
    if (isset($artifact['ledger'])) {
        $writes[] = [
            'path' => $artifact['ledger']['path'],
            'action' => $artifact['ledger']['action'] ?? 'create',
            'sha256' => $artifact['ledger']['digest'],
        ];
    }
    return $writes;
}

/** The canonical plan id: sha256 over the canonical ordered entries. */
function plan_id(array $writes): string
{
    return 'plan-' . sha256_hex(canonical_json($writes));
}

// ---------------------------------------------------------------------------
// 5a. The Laravel migration emitter (issue #57).
// ---------------------------------------------------------------------------

/** The closed step-kind vocabulary the emitter consumes (the plan v1 set). */
const MIGRATION_OPERATION_KINDS = [
    'create_table', 'drop_table', 'rename_table', 'add_column',
    'drop_column', 'alter_column_type', 'set_column_null',
    'set_column_default', 'add_primary_key', 'rename_constraint',
    'drop_constraint', 'add_unique', 'add_foreign_key', 'add_check',
    'drop_check', 'add_index', 'drop_index', 'create_enum_type',
    'alter_enum_type', 'enable_rls', 'disable_rls', 'create_policy',
    'drop_policy', 'create_sequence', 'alter_sequence',
    'rename_sequence', 'drop_sequence', 'create_extension',
    'backfill', 'rename_column',
];

/** The closed rollback classes (the contract spelling). */
const MIGRATION_ROLLBACK_CLASSES = [
    'reversible', 'data-loss-on-rollback', 'irreversible',
];

/** The closed effective-status vocabulary. */
const MIGRATION_STATUSES = ['ready', 'blocked', 'confirmed'];

/** The declared UTC timestamp base of the fixture pipeline (policy, never the wall clock). */
const MIGRATION_TIMESTAMP_BASE = '2026_09_27_000001';

/**
 * Decode and validate one bounded Laravel migration input document.
 * The closed shape mirrors contracts/laravel-migration-input.schema
 * exactly: unknown members refuse, every closed vocabulary is checked,
 * and every digest pin must be a canonical sha256 spelling.
 */
function decode_storage_input(string $bytes): array
{
    if (strlen($bytes) > MAX_REQUEST_BYTES) {
        throw new RequestRefusal('input-too-large');
    }
    try {
        $document = json_decode($bytes, true, 64, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        throw new RequestRefusal('input-syntax');
    }
    if (!is_json_object($document)) {
        throw new RequestRefusal('input-shape');
    }
    $required = [
        'schemaVersion', 'identity', 'engine', 'engineVersion',
        'projectId', 'baseDigest', 'candidateDigest', 'diffDigest',
        'planId', 'gated', 'backfillGated', 'effectiveStatus',
        'operations',
    ];
    $known = array_merge($required, ['historyDigest']);
    foreach (array_keys($document) as $key) {
        if (!in_array($key, $known, true)) {
            throw new RequestRefusal('input-member');
        }
    }
    foreach ($required as $key) {
        if (!array_key_exists($key, $document)) {
            throw new RequestRefusal('input-member');
        }
    }
    if ($document['schemaVersion'] !== 'lekalo/laravel-migration-input/v0.4.0'
        || $document['identity'] !== 'dev.lekalo.laravel-migration-input@0.4.0') {
        throw new RequestRefusal('input-identity');
    }
    if ($document['engine'] !== 'postgres'
        || !is_contract_version($document['engineVersion'])) {
        throw new RequestRefusal('input-engine');
    }
    foreach (['baseDigest', 'candidateDigest', 'diffDigest', 'planId'] as $key) {
        if (!is_sha256_digest($document[$key])) {
            throw new RequestRefusal('input-digest');
        }
    }
    if (array_key_exists('historyDigest', $document)
        && !is_sha256_digest($document['historyDigest'])) {
        throw new RequestRefusal('input-digest');
    }
    if (!is_bool($document['gated']) || !is_bool($document['backfillGated'])) {
        throw new RequestRefusal('input-shape');
    }
    if (!in_array($document['effectiveStatus'], MIGRATION_STATUSES, true)) {
        throw new RequestRefusal('input-status');
    }
    // The effective gate closes: a blocked document generates zero
    // bytes. The refusal is the honest state, never a partial file.
    if ($document['effectiveStatus'] === 'blocked') {
        throw new RequestRefusal('input-gated');
    }
    if (!is_array($document['operations']) || !array_is_list($document['operations'])
        || count($document['operations']) > 1024) {
        throw new RequestRefusal('input-operations');
    }
    $previous = 0;
    foreach ($document['operations'] as $operation) {
        if (!is_json_object($operation)) {
            throw new RequestRefusal('input-operation');
        }
        $operationKnown = ['ordinal', 'kind', 'statement', 'risk', 'requires', 'rollback', 'inverse'];
        $operationRequired = ['ordinal', 'kind', 'statement', 'risk', 'rollback'];
        foreach (array_keys($operation) as $key) {
            if (!in_array($key, $operationKnown, true)) {
                throw new RequestRefusal('input-member');
            }
        }
        foreach ($operationRequired as $key) {
            if (!array_key_exists($key, $operation)) {
                throw new RequestRefusal('input-operation');
            }
        }
        if (!is_int($operation['ordinal']) || $operation['ordinal'] <= $previous) {
            throw new RequestRefusal('input-order');
        }
        $previous = $operation['ordinal'];
        if (!in_array($operation['kind'], MIGRATION_OPERATION_KINDS, true)) {
            throw new RequestRefusal('input-kind');
        }
        if (!is_string($operation['statement']) || $operation['statement'] === ''
            || strlen($operation['statement']) > 4096) {
            throw new RequestRefusal('input-statement');
        }
        if (!in_array($operation['risk'], ['none', 'backfill_required', 'destructive'], true)) {
            throw new RequestRefusal('input-risk');
        }
        if (array_key_exists('requires', $operation)) {
            if (!is_array($operation['requires']) || !array_is_list($operation['requires'])) {
                throw new RequestRefusal('input-operation');
            }
            foreach ($operation['requires'] as $requirement) {
                if (!is_int($requirement) || $requirement < 1
                    || $requirement >= $operation['ordinal']) {
                    throw new RequestRefusal('input-requires');
                }
            }
        }
        if (!in_array($operation['rollback'], MIGRATION_ROLLBACK_CLASSES, true)) {
            throw new RequestRefusal('input-rollback');
        }
        if (array_key_exists('inverse', $operation)
            && (!is_string($operation['inverse']) || $operation['inverse'] === ''
                || strlen($operation['inverse']) > 4096)) {
            throw new RequestRefusal('input-inverse');
        }
        // A reversible operation carries its inverse: the plan v1
        // metadata fully determines it, so the absence would be a
        // truncated document, and an irreversible or data-loss class
        // never carries one.
        $hasInverse = array_key_exists('inverse', $operation);
        if (($operation['rollback'] === 'reversible') !== $hasInverse) {
            throw new RequestRefusal('input-inverse');
        }
    }
    return $document;
}

/**
 * Emit the deterministic Laravel migration artifacts for one bounded
 * input: one PHP class whose up() executes exactly the planned SQL in
 * dependency order and whose down() either inverts a fully reversible
 * plan or refuses before any statement, plus the append-only ledger.
 * Two runs with identical inputs are byte-identical; the artifact set
 * never contains anything but the migration file and the ledger.
 */
function emit_laravel_migrations(array $input, string $target, string $irPath): array
{
    // The effective gate closes here too: emit never renders a blocked
    // transition, defense in depth against a bypassed decode.
    if (($input['effectiveStatus'] ?? '') === 'blocked') {
        throw new RequestRefusal('input-gated');
    }
    $shortDigest = substr(sha256_hex(canonical_json($input)), 0, 12);
    $class = 'Lekalo' . strtoupper(substr(sha256_hex($shortDigest), 0, 8)) . 'Migration';
    $directory = '.lekalo/generated/php-laravel/' . $target . '/migrations';
    $filename = MIGRATION_TIMESTAMP_BASE . '_lekalo_' . $shortDigest . '.php';
    $path = $directory . '/' . $filename;
    $bytes = render_migration_class($input, $class, $target, $irPath);
    $ledgerPath = $directory . '/ledger.json';
    $entry = [
        'filename' => $filename,
        'digest' => sha256_digest($bytes),
        'bytes' => strlen($bytes),
        'planId' => $input['planId'],
        'inputDigest' => sha256_digest(canonical_json($input)),
        'baseDigest' => $input['baseDigest'],
        'candidateDigest' => $input['candidateDigest'],
        'timestampBase' => MIGRATION_TIMESTAMP_BASE,
        'gated' => $input['gated'],
        'backfillGated' => $input['backfillGated'],
        'effectiveStatus' => $input['effectiveStatus'],
    ];
    // The ledger is append-only custody: a published ledger never
    // shrinks and never rewrites an entry. The emitter reads the
    // published document, proves the append with assert_append_only,
    // and writes the merged ledger — so a second generate --apply
    // appends instead of dying write-denied on the existing file, and
    // a byte-identical regeneration replans exactly the published
    // bytes (a true no-op at apply time).
    $published = read_published_ledger($ledgerPath);
    if ($published === null) {
        $ledger = [
            'schemaVersion' => 'lekalo/laravel-migration-ledger/v0.4.0',
            'identity' => 'dev.lekalo.laravel-migration-ledger@0.4.0',
            'projectId' => $input['projectId'],
            'engine' => $input['engine'],
            'engineVersion' => $input['engineVersion'],
            'migrations' => [[
                'ordinal' => 1,
            ] + $entry],
        ];
        $ledgerBytes = canonical_json($ledger) . "\n";
        $ledgerAction = 'create';
    } else {
        $publishedMigrations = $published['migrations'];
        $alreadyPublished = null;
        $highestOrdinal = 0;
        foreach ($publishedMigrations as $publishedEntry) {
            $highestOrdinal = max($highestOrdinal, (int) $publishedEntry['ordinal']);
            if (($publishedEntry['filename'] ?? null) === $filename) {
                $alreadyPublished = $publishedEntry;
            }
        }
        if ($alreadyPublished !== null && $alreadyPublished['digest'] !== $entry['digest']) {
            // The same published filename with different bytes is a
            // custody violation: a migration file is named by its
            // input digest, so the published class must be identical.
            throw new RequestRefusal('ledger-conflict');
        }
        if ($alreadyPublished !== null) {
            // Byte-identical regeneration: the entry is already
            // published, the ledger keeps its published bytes, and the
            // planned write replays them exactly (no-op on disk).
            $ledgerBytes = file_get_contents($ledgerPath);
            if ($ledgerBytes === false) {
                throw new RequestRefusal('ledger-shape');
            }
            $ledgerAction = 'create';
        } else {
            $entry = ['ordinal' => $highestOrdinal + 1] + $entry;
            $merged = $published;
            $merged['migrations'][] = $entry;
            assert_append_only($published, $merged);
            $ledgerBytes = canonical_json($merged) . "\n";
            $ledgerAction = 'replace';
        }
    }
    return [
        'path' => $path,
        'bytes' => $bytes,
        'digest' => sha256_digest($bytes),
        'action' => 'create',
        'ledger' => [
            'path' => $ledgerPath,
            'bytes' => $ledgerBytes,
            'digest' => sha256_digest($ledgerBytes),
            'action' => $ledgerAction,
        ],
        'filename' => $filename,
    ];
}

/**
 * The published ledger of one migrations directory, or null when this
 * adapter has not published one yet. A present-but-invalid ledger
 * refuses: append-only custody cannot be proven over a corrupt
 * document, and silently starting a fresh ledger would orphan every
 * published migration.
 */
function read_published_ledger(string $path): ?array
{
    if (!is_file($path)) {
        return null;
    }
    $bytes = file_get_contents($path);
    if ($bytes === false) {
        throw new RequestRefusal('ledger-shape');
    }
    try {
        $document = json_decode($bytes, true, 64, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        throw new RequestRefusal('ledger-shape');
    }
    if (!is_json_object($document)
        || !isset($document['migrations'])
        || !is_array($document['migrations'])
        || !array_is_list($document['migrations'])) {
        throw new RequestRefusal('ledger-shape');
    }
    return $document;
}

/**
 * Render one Laravel migration class: strict types, an anonymous
 * class extending Illuminate\Database\Migrations\Migration, up() as
 * sequential DB::statement calls of the planned SQL, and down() from
 * the typed reverse plan or an explicit refusal. The SQL travels as
 * single-quoted PHP literals byte-for-byte — quotes, backslashes,
 * dollar signs, and Unicode survive exactly — never through eval or
 * string interpolation.
 */
function render_migration_class(array $input, string $class, string $target, string $irPath): string
{
    $lines = [];
    $lines[] = '<?php';
    $lines[] = '';
    $lines[] = '// generated by ' . ADAPTER_ID . '@' . ADAPTER_VERSION . ' (issue #57)';
    $lines[] = 'declare(strict_types=1);';
    $lines[] = '';
    $lines[] = '// target: ' . $target;
    $lines[] = '// engine: postgres ' . $input['engineVersion'];
    $lines[] = '// project: ' . $input['projectId'];
    $lines[] = '// plan: ' . $input['planId'];
    $lines[] = '// input: ' . $irPath;
    $lines[] = '';
    $lines[] = 'use Illuminate\Database\Migrations\Migration;';
    $lines[] = 'use Illuminate\Database\Schema\Blueprint;' ;
    $lines[] = 'use Illuminate\Support\Facades\DB;';
    $lines[] = '';
    $lines[] = '/**';
    $lines[] = ' * Lekalo storage transition: ' . count($input['operations']) . ' operation(s),';
    $lines[] = ' * effective gate ' . $input['effectiveStatus'] . '.';
    $lines[] = ' */';
    $lines[] = 'return new class extends Migration';
    $lines[] = '{';
    $lines[] = '    /** Execute the planned transition in dependency order. */';
    $lines[] = '    public function up(): void';
    $lines[] = '    {';
    foreach ($input['operations'] as $operation) {
        $literal = php_single_quote($operation['statement']);
        $lines[] = '        DB::statement(\'' . $literal . '\');';
    }
    $lines[] = '    }';
    $lines[] = '';
    $reverses = [];
    foreach ($input['operations'] as $operation) {
        $reverses[] = $operation['rollback'] === 'reversible'
            ? $operation['inverse']
            : null;
    }
    $fullyReversible = !in_array(null, $reverses, true);
    $lines[] = '    /** Reverse the transition; refuses before any statement when unsafe. */';
    $lines[] = '    public function down(): void';
    $lines[] = '    {';
    if ($fullyReversible) {
        foreach (array_reverse($reverses) as $inverse) {
            $literal = php_single_quote($inverse);
            $lines[] = '        DB::statement(\'' . $literal . '\');';
        }
    } else {
        $lines[] = '        // The plan carries irreversible or data-loss-on-rollback';
        $lines[] = '        // operations: the rollback refuses before the first statement.';
        $lines[] = '        throw new \RuntimeException(\'lekalo: rollback is not safe for this migration\');';
    }
    $lines[] = '    }';
    $lines[] = '};';
    return implode("\n", $lines) . "\n";
}

/**
 * One single-quoted PHP string literal preserving the exact SQL bytes:
 * backslash and single quote escape, everything else travels raw
 * (dollar signs, double quotes, Unicode). No eval, no interpolation.
 */
function php_single_quote(string $sql): string
{
    return str_replace(["\\", "'"], ['\\\\', "\\'"], $sql);
}

/**
 * The append-only custody check: a published migration ledger never
 * loses entries and never rewrites a published file. An existing
 * ledger accepts only a byte-identical regeneration (no-op) or an
 * append with a strictly higher ordinal; any other shape refuses.
 */
function assert_append_only(array $existing, array $next): void
{
    if (!is_json_object($existing)
        || !isset($existing['migrations'])
        || !is_array($existing['migrations'])
        || !array_is_list($existing['migrations'])) {
        throw new RequestRefusal('ledger-shape');
    }
    $published = $existing['migrations'];
    $nextMigrations = $next['migrations'];
    $count = count($published);
    if (count($nextMigrations) < $count) {
        throw new RequestRefusal('ledger-shrunk');
    }
    foreach ($published as $index => $entry) {
        if ($nextMigrations[$index] !== $entry) {
            throw new RequestRefusal('ledger-rewritten');
        }
    }
    $previous = 0;
    foreach ($nextMigrations as $entry) {
        if (!is_int($entry['ordinal']) || $entry['ordinal'] <= $previous) {
            throw new RequestRefusal('ledger-order');
        }
        $previous = $entry['ordinal'];
    }
}

// ---------------------------------------------------------------------------
// 6. Response envelope construction.
// ---------------------------------------------------------------------------

/** The evidence identity every response binds itself to. */
function adapter_identity(): array
{
    return [
        'id' => ADAPTER_ID,
        'version' => ADAPTER_VERSION,
        'digest' => sha256_digest(ADAPTER_ID . '@' . ADAPTER_VERSION),
    ];
}

/**
 * The capability map of this kernel (issue #28 fluent surface), extended
 * for #55 with the analysis seam identity. The seam reports the injected
 * analyzer's capabilities; the wire named-capability map stays unchanged
 * (`scan.symbols` remains unsupported: the bounded scan wire cannot
 * carry a full native graph — see scan_response).
 */
function describe_capabilities(?Analyzer $analyzer = null): array
{
    $analyzer ??= new FakeAnalyzer();
    // The operations and routes modules carry the closed root constants
    // the declared read scopes name; load before describing.
    load_route_modules();
    return [
        'adapter' => adapter_identity(),
        'protocol_versions' => SUPPORTED_VERSIONS,
        'operations' => OPERATIONS,
        'transports' => ['stdin', 'file'],
        'targets' => [TARGET_TOKEN],
        'profiles' => [PROFILE_TOKEN, STRICT_PROFILE_TOKEN],
        'read_scopes' => [
            '.lekalo/cache/**',
            '.lekalo/ir/**',
            '.lekalo/import/**',
            'app/**',
            'src/**',
            'composer.json',
            'composer.lock',
            'lekalo/php-test-port.json',
            'lekalo/scenarios/**',
            'lekalo/types/**',
            // Issue #59: the operations input home, the managed
            // operations root, and the consumer scaffold home are read
            // back for the drift, custody, and checked-join gates.
            'lekalo/operations/**',
            '.lekalo/generated/php-laravel/operations/**',
            'app/lekalo-operations/**',
            // Issue #60: the routes input home and the managed routes
            // root are read back for the drift and checked-join gates;
            // the staged transport and OpenAPI evidence rides the
            // `.lekalo/cache/**` scope.
            'lekalo/routes/**',
            '.lekalo/generated/php-laravel/routes/**',
            // Managed types and the scaffold home are read back for the
            // drift and checked-custody gates: write authority only
            // reveals a staged output's shape, so reading the bytes an
            // exchange verifies requires these explicit scopes (issue
            // #58).
            '.lekalo/generated/php-laravel/types/**',
            'app/lekalo-types/**',
        ],
        'write_scopes' => array_merge(
            ['.lekalo/generated/php-laravel/**'],
            SCENARIO_WRITE_SCOPES,
            [PHP_SCAFFOLD_SCOPE],
            [PHP_TYPES_SCAFFOLD_ROOT . '/**'],
            [PHP_OPERATIONS_SCAFFOLD_ROOT . '/**'],
        ),
        'progress' => false,
        'ir_versions' => [IR_VERSION],
        'capabilities' => array_merge(DECLARED_CAPABILITIES, [
            'generate.operations' => 'partial',
            'verify.operations' => 'partial',
            'generate.routes' => 'partial',
            'verify.routes' => 'partial',
        ]),
        // The advisory bound mirrors the kernel's real write-plan file
        // cap (MAX_WRITE_FILES): a declared constraint never exceeds an
        // internally enforced one.
        'constraints' => ['max_entries' => MAX_WRITE_FILES],
    ];
}

/**
 * The analyzer capability identity (#55). Deliberately NOT part of the
 * closed describe envelope: the v0.3.2 wire `Capabilities` struct is
 * closed (`deny_unknown_fields`), so the seam identity travels through
 * internal evidence instead of undeclared extra members. Exposed as a
 * kernel function for evidence rendering and tests.
 */
function analysis_seam_identity(?Analyzer $analyzer = null): array
{
    $analyzer ??= new FakeAnalyzer();
    return $analyzer->capabilities();
}

/**
 * Build the response envelope for `request`; the pairing rules (an
 * error response carries only the error member and vice versa) are
 * enforced by construction in the dispatch paths.
 */
function build_response(array $request, array $payload): array
{
    $envelope = [
        'protocol' => PROTOCOL_TOKEN,
        'protocol_version' => $request['protocol_version'],
        'operation' => $request['operation'],
        'request_id' => $request['request_id'],
        'status' => isset($payload['error']) ? 'error' : 'ok',
        'evidence' => ['adapter' => adapter_identity()],
    ];
    if (isset($payload['evidence_plan_id'])) {
        $envelope['evidence']['plan_id'] = $payload['evidence_plan_id'];
    }
    foreach (['capabilities', 'result', 'writes', 'progress', 'error'] as $key) {
        if (array_key_exists($key, $payload)) {
            $envelope[$key] = $payload[$key];
        }
    }
    return $envelope;
}

/** The fixed in-envelope unsupported refusal for absent operations. */
function unsupported_response(array $request, string $code = 'operation-unsupported'): array
{
    return build_response($request, [
        'error' => [
            'class' => 'unsupported',
            'code' => $code,
            'message' => 'this kernel does not implement the requested operation',
            'retryable' => false,
            'partial' => false,
        ],
    ]);
}

// ---------------------------------------------------------------------------
// 7. Dispatch: one validated request to one response.
// ---------------------------------------------------------------------------

/**
 * The declared read scopes this kernel scans (and no others). The
 * import home rides the #55 read scope so the staged receipt document
 * is a real observable scan entry — the evidence join target.
 */
const SCAN_ROOTS = ['.lekalo/ir', '.lekalo/cache', '.lekalo/import'];
/** The maximum scan entries one result may carry (mirrors the wire bound). */
const MAX_SCAN_ENTRIES = 10000;

/**
 * The scan exchange: a real read-only enumeration of the staged view's
 * declared read roots, never a canned entry list. Every returned path
 * was observed on the filesystem under a declared scope; the roots are
 * pruned by the same lexical grammar the core enforces, and the result
 * stays inside the wire bounds (a fuller inventory is `truncated`,
 * never silently cut).
 *
 * Issue #55 evidence join: the receipt's native evidence lives in the
 * target-project path domain (`app/Models/User.php`), disjoint from the
 * enumerated IR/cache roots (`.lekalo/**`). The join therefore attaches
 * the bounded evidence projection to the one governance row the receipt
 * owns inside the scanned domain: the receipt document itself
 * (`.lekalo/import/mago/receipt.json`). Source rows stay in the receipt
 * where their native paths are legal.
 */
function scan_response(array $request, ?Analyzer $analyzer = null): array
{
    $evidenceForReceipt = null;
    if ($analyzer !== null) {
        $outcome = $analyzer->analyze();
        if ($outcome->isOk()) {
            $evidenceForReceipt = mago_receipt_wire_evidence($outcome);
        }
    }
    $entries = [];
    $truncated = false;
    foreach (SCAN_ROOTS as $root) {
        if (!is_dir($root)) {
            continue;
        }
        $iterator = new RecursiveIteratorIterator(
            new RecursiveDirectoryIterator($root, FilesystemIterator::SKIP_DOTS),
            RecursiveIteratorIterator::LEAVES_ONLY,
        );
        foreach ($iterator as $file) {
            if (!$file->isFile()) {
                continue;
            }
            $path = str_replace('\\', '/', $file->getPathname());
            if (!is_logical_path($path)) {
                continue;
            }
            if (count($entries) >= MAX_SCAN_ENTRIES) {
                $truncated = true;
                break 2;
            }
            // The wire `kind` is a free bounded token: IR/cache bytes are
            // 'ir'; the staged provider receipt is not IR, so it carries
            // its honest 'evidence' spelling.
            $kind = str_starts_with($path, '.lekalo/import/') ? 'evidence' : 'ir';
            $entry = [
                'path' => $path,
                'kind' => $kind,
            ];
            if ($evidenceForReceipt !== null && $path === MAGO_RECEIPT_PATH) {
                $entry['evidence'] = $evidenceForReceipt;
            }
            $entries[] = $entry;
        }
    }
    // Canonical entry order: sorted by path (the closed wire keeps the
    // core's plan validator byte-comparable; scan results sort the same).
    usort($entries, static fn (array $a, array $b): int => strcmp($a['path'], $b['path']));
    return build_response($request, [
        'result' => [
            'entries' => $entries,
            'truncated' => $truncated,
        ],
    ]);
}

/**
 * Project the receipt's symbols and relations into the ONE bounded wire
 * evidence record that scan attaches to the receipt document itself
 * (`.lekalo/import/mago/receipt.json`): a signature digest over the
 * canonical projection (covering every symbol signature the receipt
 * carries) plus up to eight outbound references sampled deterministically
 * from the receipt's relation rows. A relation whose source symbol is
 * unknown cannot be attributed, so it drops out of the bounded
 * projection while remaining in the receipt. Any over-bound or unjoined
 * remainder drops the WHOLE claim instead of publishing a partial graph
 * under an exact-looking signature.
 *
 * @return array<string, mixed>|null null when nothing is projectable
 */
function mago_receipt_wire_evidence(AnalysisOutcome $outcome): ?array
{
    $signatures = [];
    $identityKnown = [];
    foreach ($outcome->symbols as $symbol) {
        $identityKnown[(string) $symbol['identity']] = true;
        if (isset($symbol['signature'])) {
            $signatures[(string) $symbol['path']] = (string) $symbol['signature'];
        }
    }
    ksort($signatures);
    $references = [];
    $unjoined = 0;
    $overflow = false;
    foreach ($outcome->relations as $relation) {
        $source = (string) $relation['from'];
        if (!isset($identityKnown[$source])) {
            $unjoined++;
            continue;
        }
        if (count($references) >= 8) {
            $overflow = true;
            break;
        }
        $references[] = [
            'target' => (string) $relation['to'],
            'role' => mago_wire_role((string) $relation['role']),
            'confidence' => mago_wire_confidence((string) $relation['confidence']),
        ];
    }
    // Nothing projectable: honest absence, never an empty claim.
    if ($signatures === [] || $references === [] || $overflow || $unjoined > 0) {
        return null;
    }
    $signature = sha256_digest(canonical_json(array_values($signatures)));
    return [
        'signature' => $signature,
        'references' => $references,
    ];
}

/** Map a receipt role onto the closed wire role set. */
function mago_wire_role(string $role): string
{
    return match ($role) {
        'reads' => 'read',
        'writes' => 'update',
        'calls' => 'call',
        default => 'reference',
    };
}

/** Map a receipt confidence onto the closed wire confidence set. */
function mago_wire_confidence(string $confidence): string
{
    return in_array($confidence, ['exact', 'high', 'medium', 'low', 'unknown'], true)
        ? $confidence
        : 'unknown';
}

/**
 * The validate exchange (#55): the analysis seam's verdict on the
 * staged view. Without a receipt (unavailable) the #54 empty success is
 * preserved — no analyzer, no claims either way. An incompatible or
 * failed analysis refuses with an explicit in-envelope error so a stale
 * receipt can never dress up as a pass. Findings are advice; safe fixes
 * are metadata only and are never applied by any operation.
 *
 * Profile: the default profile reports the analysis seam's recorded
 * diagnostics verbatim; the strict profile additionally evaluates the
 * Lekalo strict-profile predicates over the receipt evidence. A strict
 * request with an UNAVAILABLE receipt is not a silent clean pass: every
 * strict rule without prerequisite evidence emits its explicit
 * evidence-unsupported row (the #54 empty success can only stand for
 * the default profile). Only the declared profiles reach this function.
 */
function validate_response(array $request, ?Analyzer $analyzer = null): array
{
    // A scenario document takes the scenario gate (issue #56); a types
    // document takes the type-mapping gate (issue #58); every other
    // input keeps the analysis-seam gate (#55).
    $irPath = is_string($request['ir_path'] ?? null) ? $request['ir_path'] : '';
    if (is_scenario_ir_path($irPath)) {
        return scenario_validate_response($request);
    }
    if (resolve_routes_request($request) !== null) {
        return routes_validate_response(resolve_routes_request($request));
    }
    if (resolve_operations_request($request) !== null) {
        return operations_validate_response(resolve_operations_request($request));
    }
    if (resolve_types_request($request) !== null) {
        return types_validate_response(resolve_types_request($request));
    }
    $analyzer ??= new FakeAnalyzer();
    // Profile closure: only the two declared spellings are meaningful;
    // anything else is an in-envelope invalid refusal, never a silent
    // downgrade to default (a typo must not silently change the gate).
    $profile = $request['profile'] ?? PROFILE_TOKEN;
    if ($profile !== PROFILE_TOKEN && $profile !== STRICT_PROFILE_TOKEN) {
        return build_response($request, [
            'error' => [
                'class' => 'invalid',
                'code' => 'profile-undeclared',
                'message' => 'the requested profile is not declared by this adapter',
                'retryable' => false,
                'partial' => false,
            ],
        ]);
    }
    $outcome = $analyzer->analyze();
    if (!$outcome->isOk() && $outcome->state !== 'unavailable') {
        return build_response($request, [
            'error' => mago_analysis_error($outcome),
        ]);
    }
    $strict = ($request['profile'] ?? PROFILE_TOKEN) === STRICT_PROFILE_TOKEN;
    $findings = [];
    if ($outcome->isOk()) {
        foreach ($outcome->diagnostics as $row) {
            $findings[] = mago_wire_finding($row);
        }
        if ($strict) {
            $evaluated = strict_profile_evaluate($outcome->diagnostics, $outcome->symbols);
            foreach ($evaluated['findings'] as $row) {
                $findings[] = mago_wire_finding($row);
            }
            foreach ($evaluated['unsupported'] as $id) {
                $findings[] = mago_wire_finding(strict_unsupported_diagnostic($id));
            }
        }
    } elseif ($strict) {
        // Strict + unavailable: no evidence is never a clean strict
        // pass — every rule row without prerequisite evidence reports
        // its explicit unsupported diagnostic.
        foreach (array_keys(STRICT_RULES) as $id) {
            $findings[] = mago_wire_finding(strict_unsupported_diagnostic((string) $id));
        }
    }
    return build_response($request, [
        'result' => ['ok' => true, 'findings' => $findings],
    ]);
}

/**
 * The verify exchange (#55): the same analysis-seam gate as validate.
 * Mago success never satisfies scenario verification — a green Mago run
 * cannot masquerade as a verified scenario: scenario documents verify
 * through the dedicated scenario drift gate (#56), which is what the
 * `verify.scenarios` capability names.
 */
function verify_response(array $request, ?Analyzer $analyzer = null): array
{
    // Scenario documents verify through the scenario drift gate (issue
    // #56); types documents through the type drift/join gate (issue
    // #58); everything else keeps the analysis-seam gate (#55).
    $irPath = is_string($request['ir_path'] ?? null) ? $request['ir_path'] : '';
    if (is_scenario_ir_path($irPath)) {
        return scenario_verify_response($request);
    }
    if (resolve_routes_request($request) !== null) {
        return routes_verify_response(resolve_routes_request($request));
    }
    if (resolve_operations_request($request) !== null) {
        return operations_verify_response(resolve_operations_request($request));
    }
    if (resolve_types_request($request) !== null) {
        return types_verify_response(resolve_types_request($request));
    }
    return validate_response($request, $analyzer);
}

/**
 * Clamp UTF-8 text to at most `$limit` codepoints without ever cutting
 * mid-codepoint: PHP `substr` counts BYTES, so a byte clamp on multibyte
 * text (em-dashes, smart quotes — legal receipt message content) yields
 * invalid UTF-8 and `json_encode` would refuse the whole envelope. The
 * wire bound counts codepoints, so back off to the last complete
 * character boundary at or before the byte position that holds
 * `$limit` characters.
 */
function utf8_safe_clamp(string $text, int $limit): string
{
    if (strlen($text) <= $limit) {
        return $text;
    }
    // Walk codepoints up to the limit; O(n) in the clamped prefix.
    $offset = 0;
    $count = 0;
    $length = strlen($text);
    while ($offset < $length && $count < $limit) {
        $byte = ord($text[$offset]);
        $offset += $byte < 0x80 ? 1 : ($byte < 0xE0 ? 2 : ($byte < 0xF0 ? 3 : 4));
        $count++;
    }
    return substr($text, 0, $offset);
}

/**
 * One in-envelope error for a non-unavailable failed analysis:
 * infrastructure class, non-retryable as-is (the runner must refresh
 * the receipt; retrying the kernel request cannot fix staleness). The
 * message is clamped to the wire's 256-codepoint bound, codepoint-safe.
 *
 * @return array<string, mixed>
 */
function mago_analysis_error(AnalysisOutcome $outcome): array
{
    return [
        'class' => 'infrastructure',
        'code' => 'analysis-' . $outcome->state,
        'message' => utf8_safe_clamp('analyzer evidence is ' . $outcome->state . ': ' . (string) ($outcome->reason ?? 'unspecified'), 256),
        'retryable' => false,
        'partial' => false,
    ];
}

/**
 * One normalized strict-profile row rendered as the bounded wire
 * finding. Wire rules: `path` must be a lowercase logical path, so a
 * native evidence path (Laravel-cased, e.g. `app/Models/User.php`) is
 * lowercased for the wire grammar — and the EXACT native spelling is
 * preserved by leading the detail text, because on case-sensitive
 * filesystems the lowercased spelling points at a nonexistent file.
 * Two cased siblings (User.php / user.php in one directory — not legal
 * PSR-4, but possible) collide onto one lowercase path; the detail
 * still distinguishes them and the receipt keeps both rows exactly.
 * `detail` is clamped to the wire's 128-codepoint bound, codepoint-safe.
 *
 * @param array<string, mixed> $row
 * @return array<string, mixed>
 */
function mago_wire_finding(array $row): array
{
    $nativePath = (string) $row['path'];
    $path = $nativePath;
    if (!is_logical_path($path)) {
        // Native evidence path → logical path: lowercase segments,
        // traversal already refused by the receipt decoder.
        $path = implode('/', array_map('strtolower', explode('/', $path)));
        if (!is_logical_path($path)) {
            $path = '.lekalo/import/mago/unmappable-finding.json';
        }
    }
    $nativePrefix = $path === $nativePath ? '' : $nativePath . ' — ';
    $detail = utf8_safe_clamp(
        $nativePrefix . (string) $row['original_code'] . ': ' . (string) ($row['message'] ?? $row['rule']),
        128,
    );
    return [
        'path' => $path,
        'code' => (string) $row['rule'],
        'detail' => $detail,
    ];
}

function dispatch(array $request, ?Analyzer $analyzer = null): array
{
    $analyzer ??= new FakeAnalyzer();
    $operation = $request['operation'];
    if ($operation === 'describe') {
        $capabilities=describe_capabilities($analyzer);
        if ($request['protocol_version']==='0.3.2') { $capabilities['operations']=array_values(array_filter($capabilities['operations'],static fn(string $op): bool => $op!=='lint'));unset($capabilities['capabilities']['lint.ai-readability']); }
        return build_response($request, ['capabilities' => $capabilities]);
    }
    switch ($operation) {
        case 'lint':
            if (!function_exists('lint_collect')) {require_once __DIR__ . '/ai-lint.php';}
            return build_response($request,['result'=>['lint_evidence'=>lint_collect($request)]]);
        case 'scan':
            return scan_response($request, $analyzer);
        case 'bind':
            return build_response($request, [
                'result' => [
                    'bindings' => [[
                        'module' => 'planner',
                        'target' => $request['target'],
                        'profile' => $request['profile'],
                    ]],
                ],
            ]);
        case 'validate':
        case 'validate':
            return validate_response($request, $analyzer);
        case 'verify':
            return verify_response($request, $analyzer);
        case 'generate':
            return generate_response($request);
        case 'plan-clean':
            return plan_clean_response($request);
        case 'clean':
            return clean_response($request);
        case 'plan-native':
            return plan_native_response($request);
        default:
            return unsupported_response($request);
    }
}

/**
 * The scenario validate/verify exchange over the compiled IR evidence
 * (issue #56): the same read-and-map path generate uses, but no writes
 * ever result. Verify recomputes the expected scenario files and
 * reports one `scenario.drift` finding per drifted, missing, or
 * unreadable file; validate reports the mapper's typed findings (an
 * empty set is an honest empty findings answer).
 */
function scenario_validate_response(array $request): array
{
    $mapped = read_and_map_scenario($request);
    if (isset($mapped['error'])) {
        return $mapped['error'];
    }
    return build_response($request, ['result' => [
        'ok' => true,
        'findings' => array_map(
            static fn (array $finding): array => [
                'path' => $finding['symbol'] ?? ($finding['detail'] ?? 'scenario'),
                'code' => $finding['code'],
                'detail' => $finding['detail'] ?? null,
            ],
            $mapped['findings'],
        ),
    ]]);
}

function scenario_verify_response(array $request): array
{
    $mapped = read_and_map_scenario($request);
    if (isset($mapped['error'])) {
        return $mapped['error'];
    }
    $findings = [];
    foreach ($mapped['findings'] as $finding) {
        $findings[] = [
            'path' => $finding['symbol'] ?? ($finding['detail'] ?? 'scenario'),
            'code' => $finding['code'],
            'detail' => $finding['detail'] ?? null,
        ];
    }
    // The checked-binding join (Node parity): every `mode: checked`
    // binding must join against the observed scan index. An absent
    // index is legal silence — the join has nothing to say.
    $indexText = read_view_file(PHP_OBSERVED_INDEX_PATH);
    $index = null;
    if ($indexText !== null) {
        try {
            $index = json_decode($indexText, true, 512, JSON_THROW_ON_ERROR);
        } catch (JsonException) {
            $index = null;
        }
    }
    foreach (php_join_checked_bindings($mapped['scenario'] ?? null, $index) as $finding) {
        $findings[] = [
            'path' => $finding['symbol'] ?? 'binding',
            'code' => $finding['code'],
            'detail' => $finding['detail'] ?? null,
        ];
    }
    foreach ($mapped['files'] as $file) {
        if (($file['frozen'] ?? false) === true) {
            // Scaffold-once custody: the user-owned test is
            // existence-checked only — its bytes are the user's. A
            // missing test with a surviving marker is a removed
            // scaffold (actionable); with the marker also gone the
            // drift finding on the map already says enough.
            if (!is_file($file['path']) && is_file($file['marker'])) {
                $findings[] = [
                    'path' => $file['path'],
                    'code' => 'scenario.scaffold-missing',
                    'detail' => 'scaffolded-test-removed',
                ];
            }
            continue;
        }
        $expectedDigest = $file['digest'];
        $actual = is_file($file['path']) ? hash_file('sha256', $file['path']) : false;
        if ($actual === false) {
            $findings[] = ['path' => $file['path'], 'code' => 'scenario.drift', 'detail' => 'missing'];
        } elseif ($actual !== substr($expectedDigest, 7)) {
            $findings[] = ['path' => $file['path'], 'code' => 'scenario.drift', 'detail' => 'drifted'];
        }
    }
    return build_response($request, ['result' => ['ok' => true, 'findings' => $findings]]);
}

/**
 * The generate exchange: a dry run plans the deterministic write set
 * (existence-probing create semantics); an apply echoes the pending
 * plan id and writes exactly those bytes. Applied and declared bytes
 * are one deterministic function of the request.
 */
function generate_response(array $request): array
{
    $artifact = deterministic_generation($request);
    $writes = deterministic_writes($artifact);
    if ($artifact['findings'] !== []) {
        // Capability honesty: a compile-time finding vetoes every write.
        // The closed v0.3.2 generate result carries no findings member
        // (the client's completeness gate requires `result` to be
        // absent), so the veto is the bounded in-envelope error form:
        // the first sorted finding code names the refusal class and the
        // message carries its bounded detail. The envelope carries no
        // writes member at all — the client admits writes on an error
        // only with partial=true, and a veto is not a partial success —
        // and no plan authority: there is nothing to apply.
        $first = $artifact['findings'][0];
        $detail = (string) ($first['detail'] ?? $first['code']);
        return build_response($request, [
            'error' => [
                'class' => 'invalid',
                'code' => (string) $first['code'],
                'message' => utf8_safe_clamp($detail, 256),
                'retryable' => false,
                'partial' => false,
            ],
        ]);
    }
    if (($request['dry_run'] ?? null) === false) {
        // The apply authority is the client's pending binding, never a
        // kernel-recomputed plan id: the binding identity mixes server-
        // side context and observed-state inputs the child never sees.
        // The kernel refuses a malformed echo and otherwise treats the
        // echoed id as the pending authority, exactly like the
        // reference fixtures do.
        $requested = $request['plan_id'] ?? '';
        if (!is_plan_id($requested)) {
            return build_response($request, [
                'error' => [
                    'class' => 'conflict',
                    'code' => 'plan-mismatch',
                    'message' => 'the echoed plan id does not match the pending plan authority',
                    'retryable' => false,
                    'partial' => false,
                ],
            ]);
        }
        $files = $artifact['files']
            ?? [['path' => $artifact['path'], 'bytes' => $artifact['bytes'], 'digest' => $artifact['digest']]];
        if (isset($artifact['ledger'])) {
            // The migration ledger's exact bytes ride beside the
            // migration file so the apply loop can verify both.
            $files[] = [
                'path' => $artifact['ledger']['path'],
                'bytes' => $artifact['ledger']['bytes'],
                'digest' => $artifact['ledger']['digest'],
            ];
        }
        apply_writes($writes, $files);
    }
    return build_response($request, [
        'writes' => $writes,
        'evidence_plan_id' => $request['plan_id'] ?? plan_id($writes),
    ]);
}

/**
 * The plan-clean exchange: deletions only, over the owned artifact.
 * The scaffold scope is user-owned and never enters a delete plan.
 */
function plan_clean_response(array $request): array
{
    $writes = deterministic_writes(deterministic_generation($request));
    // Published migration artifacts are append-only custody (issue
    // #57): the ledger records them, and a generic clean confirmation
    // never retires them. The plan skips them — the deletion plan
    // covers only non-retained owned artifacts — so an orphan sweep
    // can never rewrite migration history. The scaffold scope is
    // user-owned for the same reason (issue #56).
    $plan = [];
    foreach ($writes as $entry) {
        if (retained_artifact($entry['path'])
            || scope_covers(PHP_SCAFFOLD_SCOPE, $entry['path'])
            || scope_covers(types_scaffold_scope(), $entry['path'])
            || scope_covers(operations_scaffold_scope(), $entry['path'])) {
            continue;
        }
        $plan[] = ['path' => $entry['path'], 'action' => 'delete'];
    }
    return build_response($request, [
        'writes' => $plan,
        'evidence_plan_id' => plan_id($plan),
    ]);
}

/**
 * The user-owned scaffold scope of operations generation (issue #59):
 * the closed consumer root is recognized by path convention, so a
 * policy cannot silently move a scaffold under an unrecognized root.
 */
function operations_scaffold_scope(): string
{
    load_operation_modules();
    return PHP_OPERATIONS_SCAFFOLD_ROOT . '/**';
}

/**
 * The user-owned scaffold scope of type generation (issue #58): the
 * closed consumer root is recognized by path convention, so a policy
 * cannot silently move a scaffold under an unrecognized root.
 */
function types_scaffold_scope(): string
{
    load_type_modules();
    return PHP_TYPES_SCAFFOLD_ROOT . '/**';
}

/**
 * Whether one owned artifact path is retained custody: anything under
 * a migrations directory (migration classes and the ledger) is
 * append-only history and never deletable through generic clean.
 */
function retained_artifact(string $path): bool
{
    return str_contains($path, '/migrations/');
}

/** The clean apply: delete exactly the planned paths, echo the plan id. */
function clean_response(array $request): array
{
    $writes = deterministic_writes(deterministic_generation($request));
    // Retained custody mirrors the plan: a migration or ledger path
    // refuses the apply outright instead of silently surviving. The
    // scaffold scope is excluded — never silently kept, never deleted.
    foreach ($writes as $entry) {
        if (retained_artifact($entry['path'])) {
            return build_response($request, [
                'error' => [
                    'class' => 'conflict',
                    'code' => 'retained-artifact',
                    'message' => 'published migrations and their ledger are append-only; clean never deletes them',
                    'retryable' => false,
                    'partial' => false,
                ],
            ]);
        }
    }
    $plan = [];
    foreach ($writes as $entry) {
        if (scope_covers(PHP_SCAFFOLD_SCOPE, $entry['path'])
            || scope_covers(types_scaffold_scope(), $entry['path'])
            || scope_covers(operations_scaffold_scope(), $entry['path'])) {
            continue;
        }
        $plan[] = ['path' => $entry['path'], 'action' => 'delete'];
    }
    $requested = $request['plan_id'] ?? '';
    // The apply authority is the client's pending binding (see the
    // generate apply note): the echo is shape-checked, never recomputed.
    if (!is_plan_id($requested)) {
        return build_response($request, [
            'error' => [
                'class' => 'conflict',
                'code' => 'plan-mismatch',
                'message' => 'the echoed plan id does not match the pending plan authority',
                'retryable' => false,
                'partial' => false,
            ],
        ]);
    }
    foreach ($plan as $entry) {
        delete_write($entry['path']);
    }
    return build_response($request, [
        'writes' => $plan,
        'evidence_plan_id' => $requested,
    ]);
}

/**
 * The read-only plan-native exchange (issue #48): the kernel returns
 * the honest unsupported refusal. Native gate planning needs a
 * workspace contract this MVP does not implement; a declared absent
 * capability is the honest state, never a fabricated plan summary.
 */
/**
 * The internal full-plan evidence of the most recent plan-native
 * dispatch in this process: the wire envelope carries only the closed
 * native_plan summary, while the digest-addressed full document is
 * internal custody (mirrors the Node extension outcome). One-shot
 * processes never observe it; in-process harnesses use it to pin the
 * exact plan bytes behind a wire answer.
 */
function native_last_plan(?array $set = null): ?array
{
    static $last = null;
    if ($set !== null) {
        $last = $set;
    }
    return $last;
}

function plan_native_response(array $request): array
{
    load_native_modules();
    $native = $request['native_request'];
    $policyDocument = native_gates_policy_document();
    // Custody: the request names the exact execution policy this
    // kernel pins — a different policy generation is never a planning
    // input (the confirmation bytes would not be the approved ones).
    // The digest is recomputed over the document with the digest
    // member absent (the plan_digest convention).
    $policyDigestInput = $policyDocument;
    unset($policyDigestInput['policy_digest']);
    $policyDigest = php_domain_digest(PHP_POLICY_DIGEST_DOMAIN, $policyDigestInput);
    if (($native['execution_policy_ref']['digest'] ?? null) !== $policyDigest) {
        return unsupported_response($request, 'execution-policy-mismatch');
    }
    // The Composer planner supports exactly one root project layout.
    $manifestBytes = native_read_project_bytes('composer.json');
    if ($manifestBytes === null) {
        return unsupported_response($request, 'composer-manifest-absent');
    }
    try {
        $composer = json_decode($manifestBytes, true, 64, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return build_response($request, ['error' => [
            'class' => 'invalid',
            'code' => 'native-plan-refused',
            'message' => 'composer-manifest-unparsable',
            'retryable' => false,
            'partial' => false,
        ]]);
    }
    if (!is_array($composer) || !isset($composer['name']) || !is_string($composer['name'])) {
        return build_response($request, ['error' => [
            'class' => 'invalid',
            'code' => 'native-plan-refused',
            'message' => 'composer-manifest-invalid',
            'retryable' => false,
            'partial' => false,
        ]]);
    }
    // The lock rides custody only (never parsed, never resolved).
    $lockBytes = native_read_project_bytes('composer.lock');
    $lockState = 'absent';
    $lockDigest = null;
    if (is_file('composer.lock')) {
        $lockState = $lockBytes === null ? 'unreadable' : 'present';
        $lockDigest = $lockBytes === null ? null : 'sha256:' . hash('sha256', $lockBytes);
    }
    $selectionBytes = native_read_project_bytes(NATIVE_SELECTION_PATH);
    if ($selectionBytes === null) {
        return unsupported_response($request, 'selection-manifest-absent');
    }
    try {
        $selectionManifest = json_decode($selectionBytes, true, 64, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return build_response($request, ['error' => [
            'class' => 'invalid',
            'code' => 'native-plan-refused',
            'message' => 'selection-manifest-unparsable',
            'retryable' => false,
            'partial' => false,
        ]]);
    }
    if (!is_array($selectionManifest)) {
        return build_response($request, ['error' => [
            'class' => 'invalid',
            'code' => 'native-plan-refused',
            'message' => 'selection-manifest-unparsable',
            'retryable' => false,
            'partial' => false,
        ]]);
    }
    // Pure verification: every confirmation is checked against the
    // exact manifest bytes read above; the whole policy must verify.
    $verification = php_verify_confirmations($policyDocument, 'native_read_project_bytes');
    if (!$verification['ok']) {
        $reason = $verification['unverifiable'][0] ?? 'confirmations-unverifiable';
        return build_response($request, ['error' => [
            'class' => 'invalid',
            'code' => 'native-plan-refused',
            'message' => substr($reason, 0, 128),
            'retryable' => false,
            'partial' => false,
        ]]);
    }
    $custody = [
        'input_manifest_digest' => (string) $native['input_manifest_digest'],
        'tool_catalog_digest' => php_tool_catalog_digest($verification['tool_catalog']),
        'capability_snapshot_digest' => (string) $native['capability_snapshot_digest'],
        'scan_ref' => (string) $native['scan_ref']['digest'],
        'observed_ref' => isset($native['observed_ref']['digest'])
            ? (string) $native['observed_ref']['digest']
            : 'sha256:' . str_repeat('0', 64),
        'profile_id' => (string) ($request['profile'] ?? PROFILE_TOKEN),
        'profile_digest' => 'sha256:' . sha256_hex((string) ($request['profile'] ?? PROFILE_TOKEN)
            . '@' . TARGET_TOKEN),
        'adapter_identity' => adapter_identity(),
    ];
    try {
        $plan = php_build_native_plan([
            'composer' => $composer,
            'composer_lock_state' => $lockState,
            'composer_lock_digest' => $lockDigest,
            'policy' => $policyDocument,
            'confirmed' => $verification['confirmed'],
            'tool_catalog' => $verification['tool_catalog'],
            'changes' => [
                'files' => is_array($native['changes']['files'] ?? null) ? $native['changes']['files'] : [],
                'symbols' => is_array($native['changes']['symbols'] ?? null) ? $native['changes']['symbols'] : [],
            ],
            'selection_manifest' => $selectionManifest,
            'custody' => $custody,
        ]);
    } catch (PhpPlanRefusal $refusal) {
        return build_response($request, ['error' => [
            'class' => 'invalid',
            'code' => 'native-plan-refused',
            'message' => substr($refusal->getMessage(), 0, 128),
            'retryable' => false,
            'partial' => false,
        ]]);
    }
    // The wire carries the closed native_plan summary only: the full
    // plan is digest-addressed custody, never a free-floating payload.
    // The in-process accessor pins the exact bytes for the host.
    native_last_plan($plan);
    return build_response($request, ['result' => [
        'ok' => true,
        'native_plan' => [
            'kind' => 'native-plan',
            'digest' => $plan['plan_digest'],
            'commands' => count($plan['commands']),
            'packages' => count($plan['workspace']['packages']),
            'completeness' => $plan['workspace']['completeness'],
            'run_eligibility' => $plan['run_eligibility']['state'],
        ],
    ]]);
}

// ---------------------------------------------------------------------------
// 8. The bounded write view inside the staged project.
// ---------------------------------------------------------------------------

/**
 * Apply the declared creates inside the core's private staged view.
 * The kernel trusts the core's sandbox for scope authority; it still
 * refuses paths outside its own declared write scope, protected homes,
 * non-logical paths, and create-on-existing, mirroring the plan
 * semantics core verifies after the child exits. Each write's declared
 * digest must match the bytes it carries: a plan/byte divergence is a
 * kernel bug, never a silent publish.
 */
function apply_writes(array $writes, array $files): void
{
    // Every planned entry writes its own exact bytes: the scenario
    // emitter ships the file list directly; the fixture and migration
    // emitters ship artifact (and ledger) bytes normalized by the
    // caller into the same file-list shape.
    $bytesByPath = [];
    foreach ($files as $file) {
        $bytesByPath[$file['path']] = $file['bytes'];
    }
    foreach ($writes as $entry) {
        $path = $entry['path'];
        if (!isset($bytesByPath[$path])) {
            throw new RequestRefusal('write-denied');
        }
        $bytes = $bytesByPath[$path];
        if (!is_logical_path($path) || protected_home($path) !== null) {
            throw new RequestRefusal('write-denied');
        }
        // The scenario home writes under the project's src tree; the
        // scaffold scopes admit the one-shot user-owned emission (the
        // scenario home and the type scaffold root); every other
        // generated artifact stays inside the runtime-owned
        // `.lekalo/generated/php-laravel/**` home.
        $inScenarioScope = false;
        foreach (SCENARIO_WRITE_SCOPES as $scope) {
            if (scope_covers($scope, $path)) {
                $inScenarioScope = true;
                break;
            }
        }
        if (!scope_covers('.lekalo/generated/php-laravel/**', $path) && !$inScenarioScope
            && !scope_covers(PHP_SCAFFOLD_SCOPE, $path)
            && !scope_covers(types_scaffold_scope(), $path)
            && !scope_covers(operations_scaffold_scope(), $path)) {
            throw new RequestRefusal('write-denied');
        }
        $bytes = $bytesByPath[$path] ?? null;
        if ($bytes === null || strlen($bytes) > MAX_FILE_BYTES) {
            throw new RequestRefusal('write-denied');
        }
        if (('sha256:' . hash('sha256', $bytes)) !== $entry['sha256']) {
            throw new RequestRefusal('write-denied');
        }
        $exists = is_file($path);
        if ($exists) {
            if (hash_equals($entry['sha256'], sha256_digest((string) file_get_contents($path)))) {
                // Byte-identical regeneration: the published bytes
                // already equal the plan, so this entry is a true
                // no-op — append-only history stays untouched.
                continue;
            }
            if (($entry['action'] ?? 'create') === 'replace') {
                // A replace is custody's append lane: only the retained
                // ledger may be overwritten, and only with the merged
                // bytes the emitter's assert_append_only proved.
                if (!retained_artifact($path)) {
                    throw new RequestRefusal('write-denied');
                }
            } elseif (retained_artifact($path)) {
                // Create-on-existing with different bytes refuses for
                // append-only custody: a published migration is never
                // silently rewritten. Every other owned path follows
                // the plan — the staged view overwrites with the
                // planned exact bytes (scenario support files are
                // per-scenario and legitimately rewritten).
                throw new RequestRefusal('write-denied');
            }
        }
        $directory = dirname($path);
        if (!is_dir($directory) && !mkdir($directory, 0777, true) && !is_dir($directory)) {
            throw new RequestRefusal('write-denied');
        }
        if (@file_put_contents($path, $bytes) === false) {
            throw new RequestRefusal('write-denied');
        }
    }
}

function delete_write(string $path): void
{
    if (!is_logical_path($path) || protected_home($path) !== null
        || scope_covers(PHP_SCAFFOLD_SCOPE, $path)
        || scope_covers(types_scaffold_scope(), $path)
        || scope_covers(operations_scaffold_scope(), $path)) {
        // The scaffold scopes are user-owned: no kernel path may delete
        // inside them, whatever plan claimed otherwise.
        throw new RequestRefusal('write-denied');
    }
    $inScenarioScope = false;
    foreach (SCENARIO_WRITE_SCOPES as $scope) {
        if (scope_covers($scope, $path)) {
            $inScenarioScope = true;
            break;
        }
    }
    if (!scope_covers('.lekalo/generated/php-laravel/**', $path) && !$inScenarioScope) {
        throw new RequestRefusal('write-denied');
    }
    if (is_file($path)) {
        @unlink($path);
    }
}

// ---------------------------------------------------------------------------
// 9. One-shot main.
// ---------------------------------------------------------------------------

/** The bounded stderr diagnostic for transport-level refusals. */
function stderr_diagnostic(string $code): string
{
    $bounded = preg_replace('/[^a-z0-9._-]+/', '-', strtolower($code)) ?? 'invalid';
    $bounded = trim($bounded, '-');
    if ($bounded === '') {
        $bounded = 'unspecified';
    }
    return json_encode(
        ['kernel' => ADAPTER_ID, 'diagnostic' => substr($bounded, 0, 128)],
        JSON_UNESCAPED_SLASHES,
    );
}

/**
 * Local metadata probe: exact adapter id/version and the PHP runtime
 * version. NOT part of the wire protocol — the wire has no
 * runtime-version slot; this stays a local argv probe exactly like the
 * Node kernel's `--version-json`.
 */
function runtime_metadata(): array
{
    return [
        'adapter' => ADAPTER_ID,
        'version' => ADAPTER_VERSION,
        'php' => PHP_VERSION,
        'entry' => 'adapter.php',
    ];
}

function main(): int
{
    $arguments = array_slice($_SERVER['argv'] ?? [], 1);
    if ($arguments === ['--version-json']) {
        fwrite(STDOUT, canonical_json(runtime_metadata()) . "\n");
        return 0;
    }
    try {
        $bytes = read_request_bytes();
        $document = decode_json_document($bytes);
        $request = validate_request_object($document);
        // The production composition: the subprocess-free evidence
        // consumer over the runner-staged receipt inside the declared
        // read view. An absent receipt reports `unavailable` — the exact
        // #54 no-claims posture — while a staged receipt flows through
        // the closed decoder and the pin/compatibility gates. The test
        // double exists for suites and gates only; no request field,
        // environment variable, or argv flag can select it here.
        $analyzer = new MagoEvidenceAnalyzer();
        fwrite(STDOUT, canonical_json(dispatch($request, $analyzer)));
        return 0;
    } catch (RequestRefusal $refusal) {
        fwrite(STDERR, stderr_diagnostic($refusal->getMessage()) . "\n");
        return 1;
    } catch (JsonException) {
        fwrite(STDERR, stderr_diagnostic('syntax') . "\n");
        return 1;
    } catch (Throwable) {
        // Any other failure refuses honestly: bounded stderr, nonzero
        // exit, never a synthetic success envelope.
        fwrite(STDERR, stderr_diagnostic('kernel') . "\n");
        return 1;
    }
}

// ----- bundled compiler module: scenario-map.php -----


/**
 * Pure Scenario IR → test-AST mapping for the scenario-test compiler
 * (issue #56, S2). A structural port of the Node mapper
 * (`adapters/node-typescript/src/scenario-map.mjs`): the same closed
 * scenario shapes resolve to the same test model, with the PHP runner
 * registry in place of the Node one.
 *
 * Inputs are one decoded Scenario IR document, one decoded compiled
 * project IR document, one decoded project test-port declaration, and
 * the profile's negotiated capability snapshot. The output is a closed
 * test model plus typed findings. No filesystem, clock, environment,
 * process, or network access happens here: the same inputs always map
 * to the same model — the determinism contract the byte-stable emitter
 * depends on.
 *
 * Honesty rules (mirrored from the Node mapper):
 * - Every scenario feature without a port surface, runner capability,
 *   or resolvable target lands in `unsupported[]` with its diagnostic
 *   — never silently dropped, never approximated, never a pass.
 * - Operation references resolve to `command` or `query` from the
 *   compiled IR, never from the name; anything else is a
 *   `scenario.operation-unresolved` compile-time finding.
 * - Concurrency race scenarios (metadata `testing.concurrency`) map to
 *   an explicit whole-scenario unsupported outcome: a serial execution
 *   never satisfies a race fixture.
 * - `unsupported` assertion kinds compile to recorded unsupported rows
 *   and can never report pass.
 */

const PHP_SCENARIO_IDENTITY = 'dev.lekalo.scenario-ir@0.2.16';
const PHP_IR_IDENTITY = 'dev.lekalo.ir@0.2.16';

/** The generated scenario-test home under the generated root. */
const PHP_SCENARIO_TESTS_DIR = 'src/generated/php-laravel/scenario-tests';

/**
 * The adapter-owned PHP port declaration path (issue #56).
 *
 * The core `lekalo/test-port` contract v0.4.0 restricts port paths to
 * `.mjs`/`.ts` modules, and its version custody is pinned to the
 * workspace product version, so a `.php` port cannot ride that family
 * without the coordinated contract successor (docs/m5/issue-56-research.md
 * S1, deliberately deferred). Until that successor lands, the PHP
 * adapter reads its own bounded sibling document `lekalo/php-test-port.json`
 * with the exact closed export-flag vocabulary of the core contract, so
 * the eventual lift is mechanical.
 */
const PHP_PORT_DOC_PATH = 'lekalo/php-test-port.json';

/** The closed identity of the adapter-owned PHP port declaration. */
const PHP_PORT_DOC_SCHEMA_VERSION = 'lekalo/php-test-port/v0.1.0';
const PHP_PORT_DOC_IDENTITY = 'dev.lekalo.php-test-port@0.1.0';

/** The declared-port class FQN grammar: PSR-4 style, bounded, no code. */
const PHP_PORT_CLASS_PATTERN = '/^[A-Za-z_][A-Za-z0-9_]*(\\\\[A-Za-z_][A-Za-z0-9_]*)*$/';

/**
 * The closed PHP runner registry. The Laratesto entry mirrors the
 * bridge's declared facts on the pinned toolchain: the Laravel suite is
 * sequential (fresh application per test), so `testing.concurrency`
 * stays deliberately absent — a race case compiles to an explicit
 * unsupported outcome, never to a serial run that would lie.
 */
const PHP_RUNNER_REGISTRY = [
    'laratesto' => [
        'capabilities' => [
            'testing.clock',
            'testing.db-refresh',
            'testing.event-capture',
            'testing.fixtures',
            'testing.http',
            'testing.session',
        ],
        'concurrency' => false,
        'eventCapture' => 'plugin',
        'syntax' => 'laratesto',
    ],
];

/** The default runner when a scenario declares no native binding. */
const PHP_DEFAULT_RUNNER = 'laratesto';

/**
 * Binding capabilities the port's `invoke` surface itself provides when
 * the dispatch enforces idempotency dedup (mirrors the Node mapper).
 */
const PHP_PORT_PROVIDED_CAPABILITIES = [
    'idempotency.durable_key',
    'idempotency.replay',
];

/** The scenario metadata key whose presence marks a concurrency race case. */
const PHP_CONCURRENCY_METADATA_KEY = 'testing.concurrency';

/** The closed given-step precondition kinds. */
const PHP_PRECONDITION_KINDS = ['state', 'fixture', 'actor', 'clock', 'id_source'];

/** The closed assertion kinds. */
const PHP_ASSERTION_KINDS = [
    'result', 'error', 'entity_state', 'emitted', 'forbidden_effect',
    'authorization', 'idempotency', 'contract_match', 'deterministic_fixture',
    'unsupported',
];

/** The closed typed-value wire kinds (scenario/value.rs). */
const PHP_VALUE_KINDS = [
    'null', 'boolean', 'integer', 'string', 'decimal', 'date', 'datetime',
    'uuid', 'uri', 'list', 'object',
];

/** The closed reference kinds (scenario/reference.rs). */
const PHP_REF_KINDS = [
    'symbol', 'operation', 'entity', 'field', 'event', 'job', 'effect',
    'error', 'requirement', 'fixture', 'actor', 'clock', 'id-source',
    'step-output', 'given-value',
];

/** IR bounds mirrored from the core (scenario/version.rs). */
const PHP_LIMITS = [
    'maxGivenSteps' => 256,
    'maxWhenSteps' => 256,
    'maxThenSteps' => 512,
    'maxTotalSteps' => 1024,
    'maxBindings' => 32,
    'maxTypedDepth' => 32,
    'maxTypedItems' => 4096,
    'maxScalarCodepoints' => 4096,
];

/** The closed port surface flags (beyond the mandatory `invoke`). */
const PHP_PORT_FLAGS = [
    'invoke', 'state', 'fixtures', 'actor', 'clock', 'ids',
    'emissions', 'effects', 'authorize', 'contractCheck', 'fixtureDigest',
    'reset',
];

// ---------------------------------------------------------------------------
// Closed-shape grammar checks (mirrors of the Node mapper predicates).
// ---------------------------------------------------------------------------

/**
 * The closed SemanticId grammar mirrored from the core
 * (scenario/id.rs): two or three dot-separated lowercase segments
 * (`[a-z][a-z0-9_]*`, ≤63 each), total ≤191 bytes, and the first
 * segment never the reserved `lekalo`/`dev`.
 */
function is_php_semantic_id(mixed $text): bool
{
    if (!is_string($text) || $text === '' || strlen($text) > 191) {
        return false;
    }
    $segments = explode('.', $text);
    $count = count($segments);
    if ($count < 2 || $count > 3) {
        return false;
    }
    foreach ($segments as $index => $segment) {
        if ($segment === '' || strlen($segment) > 63) {
            return false;
        }
        if (!preg_match('/^[a-z][a-z0-9_]*$/', $segment)) {
            return false;
        }
        if ($index === 0 && ($segment === 'lekalo' || $segment === 'dev')) {
            return false;
        }
    }
    return true;
}

/** The closed single-segment step-id grammar (scenario/id.rs::StepId). */
function is_php_step_id(mixed $text): bool
{
    return is_string($text)
        && $text !== ''
        && strlen($text) <= 64
        && preg_match('/^[a-z][a-z0-9_]*$/', $text) === 1;
}

/** The closed canonical JSON writer (byte-identical to the Node rule). */
function php_canonical_json(mixed $value): string
{
    if ($value === null) {
        return 'null';
    }
    if (is_bool($value)) {
        return $value ? 'true' : 'false';
    }
    if (is_int($value)) {
        return (string) $value;
    }
    if (is_string($value)) {
        return json_encode($value, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
    }
    if (is_array($value) && array_is_list($value)) {
        $items = array_map(__FUNCTION__, $value);
        return '[' . implode(',', $items) . ']';
    }
    if (is_array($value)) {
        $keys = array_keys($value);
        usort($keys, 'strcmp');
        $body = [];
        foreach ($keys as $key) {
            $body[] = php_canonical_json((string) $key) . ':' . php_canonical_json($value[$key]);
        }
        return '{' . implode(',', $body) . '}';
    }
    // Floats never appear in a closed scenario document: the wire keeps
    // integers as integers (or decimal spellings) and strings as strings.
    throw new LogicException('unrenderable canonical JSON value');
}

// ---------------------------------------------------------------------------
// Mapping entry point.
// ---------------------------------------------------------------------------

/**
 * Map one scenario document plus its joined context into the test model.
 *
 * `input` is `{scenario, ir, port, portPresent, profileCapabilities}`:
 * `scenario` is the decoded Scenario IR document, `ir` the decoded
 * compiled project IR evidence (null when absent), `port` the decoded
 * test-port declaration (null when `portPresent` is false), and
 * `profileCapabilities` the negotiated `[{id, support}]` snapshot (null
 * when the launch carries no resolution). Returns the closed mapper
 * outcome: `{state: "refused", refusal}` or
 * `{state: "mapped", scenarios, findings}`.
 */
function php_map_scenario(array $input): array
{
    $scenario = $input['scenario'] ?? null;
    $findings = [];
    $context = [
        'findings' => &$findings,
        'scenarioId' => is_string($scenario['scenarioId'] ?? null) ? $scenario['scenarioId'] : null,
        'operationIndex' => php_operation_index($input['ir'] ?? null),
    ];
    $shapeRefusal = php_check_scenario_shape($scenario);
    if ($shapeRefusal !== null) {
        return ['state' => 'refused', 'refusal' => $shapeRefusal, 'scenarios' => [], 'findings' => []];
    }
    $ir = $input['ir'] ?? null;
    $irDigest = $input['irDigest'] ?? null;
    $irRefOk = is_array($ir)
        && ($ir['contract'] ?? null) === PHP_IR_IDENTITY
        && isset($scenario['irRef']['digest'])
        && $irDigest !== null
        && $scenario['irRef']['digest'] === $irDigest;
    if (!$irRefOk) {
        $findings[] = [
            'code' => 'scenario.ir-ref-mismatch',
            'symbol' => $scenario['scenarioId'],
            'detail' => 'ir-ref-digest',
        ];
    }
    $runner = php_resolve_runner($scenario, $findings);
    $portSurface = php_resolve_port_surface($input, $findings);
    $unsupported = [];
    php_collect_concurrency_unsupported($scenario, $runner, $unsupported);
    php_collect_capability_gaps($scenario, $input, $runner, $unsupported);
    $model = [
        'id' => $scenario['scenarioId'],
        'version' => $scenario['scenarioVersion'],
        'summary' => $scenario['summary'],
        'projectId' => $scenario['projectId'],
        'irDigest' => $scenario['irRef']['digest'] ?? null,
        'runner' => $runner,
        'binding' => php_binding_model($scenario),
        'tags' => $scenario['tags'] ?? [],
        'unsupported' => $unsupported,
        'given' => php_map_given($scenario['given'] ?? [], $portSurface),
        'when' => php_map_when($scenario['when'] ?? [], $context, $portSurface),
        'then' => php_map_then($scenario['then'] ?? [], $context, $portSurface),
    ];
    return ['state' => 'mapped', 'scenarios' => [$model], 'findings' => $findings];
}

/** The closed-shape decoder (defense in depth over the core's custody). */
function php_check_scenario_shape(mixed $scenario): ?string
{
    if (!is_array($scenario) || array_is_list($scenario)) {
        return 'scenario-shape';
    }
    if (($scenario['schemaVersion'] ?? null) !== 'lekalo/scenario-ir/v0.2.16'
        || ($scenario['identity'] ?? null) !== PHP_SCENARIO_IDENTITY) {
        return 'scenario-identity';
    }
    foreach ([
        'projectId', 'scenarioId', 'scenarioVersion', 'summary',
        'irRef', 'modelRef', 'given', 'when', 'then', 'bindings', 'tags', 'metadata',
    ] as $key) {
        if (!array_key_exists($key, $scenario)) {
            return 'scenario-missing-field';
        }
    }
    // The comment block of every emitted test interpolates the version
    // and the summary: both must be strings before the comment-safe
    // projection runs, so a non-string wire shape can never reach the
    // emitter at all (defense in depth over the core's custody).
    foreach (['scenarioVersion', 'summary'] as $textField) {
        if (!is_string($scenario[$textField])) {
            return 'scenario-text-field';
        }
    }
    if (!is_php_semantic_id($scenario['scenarioId'])) {
        return 'scenario-id';
    }
    if (!is_array($scenario['given']) || !is_array($scenario['when']) || !is_array($scenario['then'])) {
        return 'scenario-steps-shape';
    }
    if (count($scenario['when']) === 0 || count($scenario['then']) === 0) {
        return 'scenario-empty';
    }
    if (count($scenario['given']) > PHP_LIMITS['maxGivenSteps']
        || count($scenario['when']) > PHP_LIMITS['maxWhenSteps']
        || count($scenario['then']) > PHP_LIMITS['maxThenSteps']
        || count($scenario['given']) + count($scenario['when']) + count($scenario['then'])
            > PHP_LIMITS['maxTotalSteps']) {
        return 'scenario-steps-bound';
    }
    if (!is_array($scenario['bindings']) || count($scenario['bindings']) > PHP_LIMITS['maxBindings']) {
        return 'scenario-bindings-bound';
    }
    $stepIds = [];
    foreach ([$scenario['given'], $scenario['when'], $scenario['then']] as $role) {
        foreach ($role as $step) {
            if (!is_array($step) || array_is_list($step)
                || !is_php_step_id($step['stepId'] ?? null)) {
                return 'scenario-step-id';
            }
            if (in_array($step['stepId'], $stepIds, true)) {
                return 'scenario-duplicate-step-id';
            }
            $stepIds[] = $step['stepId'];
        }
    }
    return null;
}

/** The wire-shape check for one typed value or reference leaf. */
function php_check_leaf(mixed $leaf, int $depth): ?string
{
    if (!is_array($leaf) || array_is_list($leaf)) {
        return 'leaf-shape';
    }
    if ($depth > PHP_LIMITS['maxTypedDepth']) {
        return 'leaf-depth';
    }
    if (is_string($leaf['$ref'] ?? null)) {
        return in_array($leaf['$ref'], PHP_REF_KINDS, true) ? null : 'leaf-ref-kind';
    }
    if (!is_string($leaf['type'] ?? null) || !in_array($leaf['type'], PHP_VALUE_KINDS, true)) {
        return 'leaf-value-kind';
    }
    if ($leaf['type'] === 'list') {
        if (!is_array($leaf['value'] ?? null) || count($leaf['value']) > PHP_LIMITS['maxTypedItems']) {
            return 'leaf-items';
        }
        foreach ($leaf['value'] as $item) {
            $problem = php_check_leaf($item, $depth + 1);
            if ($problem !== null) {
                return $problem;
            }
        }
        return null;
    }
    if ($leaf['type'] === 'object') {
        $map = $leaf['value'] ?? null;
        if (!is_array($map) || array_is_list($map)) {
            return 'leaf-object';
        }
        if (count($map) > PHP_LIMITS['maxTypedItems']) {
            return 'leaf-items';
        }
        foreach ($map as $child) {
            $problem = php_check_leaf($child, $depth + 1);
            if ($problem !== null) {
                return $problem;
            }
        }
        return null;
    }
    if (!array_key_exists('value', $leaf)) {
        return 'leaf-value';
    }
    if (is_string($leaf['value'])) {
        // The /./us scan returns false on malformed UTF-8: that must
        // fail closed (refuse the leaf), never compare as zero and
        // silently pass the bound.
        $codepoints = preg_match_all('/./us', $leaf['value']);
        if ($codepoints === false || $codepoints > PHP_LIMITS['maxScalarCodepoints']) {
            return 'leaf-scalar';
        }
    }
    return null;
}

// ---------------------------------------------------------------------------
// Resolution: operations, runners, port surfaces, capabilities.
// ---------------------------------------------------------------------------

/** The command/query index of the compiled IR evidence. */
function php_operation_index(mixed $ir): array
{
    $index = [];
    if (!is_array($ir) || !is_array($ir['definitions'] ?? null)) {
        return $index;
    }
    foreach ($ir['definitions'] as $definition) {
        if (!is_array($definition)) {
            continue;
        }
        if (($definition['kind'] ?? null) === 'command' || ($definition['kind'] ?? null) === 'query') {
            $index[$definition['id']] = $definition['kind'];
        }
    }
    return $index;
}

/**
 * Resolve one scenario operation reference against the compiled IR. The
 * exact definition id wins; otherwise the kind-qualified wire spelling
 * (`<module>.command.<name>` / `<module>.query.<name>`) resolves when
 * the base id is declared with exactly that kind. Anything else is
 * unresolved — never a guessed call kind.
 */
function php_resolve_operation(array $index, string $id): ?array
{
    if (isset($index[$id])) {
        return ['id' => $id, 'kind' => $index[$id]];
    }
    $segments = explode('.', $id);
    $count = count($segments);
    if ($count >= 3) {
        $kind = $segments[$count - 2];
        if ($kind === 'command' || $kind === 'query') {
            $base = implode('.', array_merge(array_slice($segments, 0, -2), [$segments[$count - 1]]));
            if (($index[$base] ?? null) === $kind) {
                return ['id' => $base, 'kind' => $kind, 'ref' => $id];
            }
        }
    }
    return null;
}

/** The resolved runner entry, or an unknown-runner finding with the default. */
function php_resolve_runner(array $scenario, array &$findings): array
{
    $nativeBinding = null;
    foreach ($scenario['bindings'] ?? [] as $binding) {
        if (is_array($binding) && ($binding['backend'] ?? null) === 'native') {
            $nativeBinding = $binding;
            break;
        }
    }
    $runnerId = is_string($nativeBinding['runner'] ?? null)
        ? $nativeBinding['runner']
        : PHP_DEFAULT_RUNNER;
    if (!isset(PHP_RUNNER_REGISTRY[$runnerId])) {
        $findings[] = [
            'code' => 'scenario.runner-unknown',
            'symbol' => $scenario['scenarioId'],
            'detail' => php_bound_token($runnerId),
        ];
        $runnerId = PHP_DEFAULT_RUNNER;
    }
    $entry = PHP_RUNNER_REGISTRY[$runnerId];
    $runner = array_merge(['id' => $runnerId], $entry);
    if (is_string($nativeBinding['runnerVersion'] ?? null)) {
        $runner['declaredVersion'] = $nativeBinding['runnerVersion'];
    }
    return $runner;
}

/**
 * Validate the adapter-owned PHP port declaration against its closed
 * shape (issue #56): bounded document, closed identity, one logical
 * `.php` path, one PSR-4 class FQN, and the exact closed export-flag
 * vocabulary of the core test-port contract (absent/other-than-true
 * means the feature compiles to an explicit unsupported diagnostic).
 * Returns the normalized `{path, class, exports}` document, or null
 * when any bound is violated. No code, no expressions, no traversal:
 * the grammar itself keeps the declaration data-only.
 */
function php_validate_port_doc(array $doc): ?array
{
    if (($doc['schema_version'] ?? null) !== PHP_PORT_DOC_SCHEMA_VERSION
        || ($doc['identity'] ?? null) !== PHP_PORT_DOC_IDENTITY
        || count($doc) !== 3
        || !is_array($doc['port'] ?? null)
        || count($doc['port']) !== 3) {
        return null;
    }
    $port = $doc['port'];
    $path = $port['path'] ?? null;
    if (!is_string($path) || $path === '' || strlen($path) > 256
        || preg_match('/^[a-zA-Z0-9][a-zA-Z0-9._\/-]*\\.php$/', $path) !== 1
        || str_contains($path, '..')) {
        return null;
    }
    $class = $port['class'] ?? null;
    if (!is_string($class) || $class === '' || strlen($class) > 256
        || preg_match(PHP_PORT_CLASS_PATTERN, $class) !== 1) {
        return null;
    }
    $exports = $port['exports'] ?? null;
    if (!is_array($exports) || ($exports['invoke'] ?? null) !== true) {
        return null;
    }
    foreach ($exports as $flag => $value) {
        if (!in_array($flag, PHP_PORT_FLAGS, true) || !is_bool($value)) {
            return null;
        }
    }
    return ['path' => $path, 'class' => $class, 'exports' => $exports];
}

/** The port surface join: every closed port flag the project declares.
 * `port` is the kernel-validated normalized document (`path`, `class`,
 * `exports`); a declaration-absent project keeps the port-missing
 * finding and an all-false surface, so every port-backed feature maps
 * to an explicit unsupported diagnostic.
 */
function php_resolve_port_surface(array $input, array &$findings): array
{
    $port = $input['port'] ?? null;
    if (($input['portPresent'] ?? false) !== true || !is_array($port)) {
        $findings[] = ['code' => 'scenario.port-missing', 'detail' => 'declaration-absent'];
        return php_empty_surface();
    }
    $exports = is_array($port['exports'] ?? null) ? $port['exports'] : null;
    if (!is_array($exports) || ($exports['invoke'] ?? null) !== true) {
        $findings[] = ['code' => 'scenario.port-shape', 'detail' => 'exports-shape'];
        return php_empty_surface();
    }
    $surface = [];
    foreach (PHP_PORT_FLAGS as $flag) {
        $surface[$flag] = ($exports[$flag] ?? null) === true;
    }
    return $surface;
}

function php_empty_surface(): array
{
    $surface = [];
    foreach (PHP_PORT_FLAGS as $flag) {
        $surface[$flag] = false;
    }
    return $surface;
}

/**
 * Concurrency race cases: a scenario that declares the concurrency
 * metadata compiles to one whole-scenario unsupported row. A serial
 * execution never satisfies a race fixture, and the Laratesto suite is
 * sequential by contract.
 */
function php_collect_concurrency_unsupported(array $scenario, array $runner, array &$unsupported): void
{
    $metadata = $scenario['metadata'] ?? null;
    if (!is_array($metadata) || !array_key_exists(PHP_CONCURRENCY_METADATA_KEY, $metadata)) {
        return;
    }
    if (in_array('testing.concurrency', $runner['capabilities'], true)) {
        return;
    }
    $unsupported[] = [
        'step' => null,
        'capability' => 'testing.concurrency',
        'reason' => 'scenario-requires-concurrency',
        'detail' => php_bound_token((string) $metadata[PHP_CONCURRENCY_METADATA_KEY]),
    ];
}

/**
 * The binding capability join: every declared binding capability must be
 * resolvable from the runner registry entry, the port dispatch surface,
 * or the negotiated profile snapshot; each gap is one unsupported row.
 */
function php_collect_capability_gaps(array $scenario, array $input, array $runner, array &$unsupported): void
{
    $profile = [];
    foreach ($input['profileCapabilities'] ?? [] as $entry) {
        if (is_array($entry) && isset($entry['id']) && isset($entry['support'])
            && $entry['support'] !== 'unsupported') {
            $profile[$entry['id']] = true;
        }
    }
    foreach ($scenario['bindings'] ?? [] as $binding) {
        if (!is_array($binding) || ($binding['backend'] ?? null) !== 'native') {
            continue; // fake-reference stays with #107
        }
        foreach (is_array($binding['capabilities'] ?? null) ? $binding['capabilities'] : [] as $capability) {
            if (!is_string($capability)) {
                continue;
            }
            if (in_array($capability, $runner['capabilities'], true)) {
                continue;
            }
            if (in_array($capability, PHP_PORT_PROVIDED_CAPABILITIES, true)) {
                continue;
            }
            if ($profile !== [] && !isset($profile[$capability])) {
                $unsupported[] = [
                    'step' => null,
                    'capability' => $capability,
                    'reason' => 'binding-capability-gap',
                    'detail' => 'profile-snapshot',
                ];
                continue;
            }
            if ($profile === []) {
                $unsupported[] = [
                    'step' => null,
                    'capability' => $capability,
                    'reason' => 'binding-capability-gap',
                    'detail' => 'runner-registry',
                ];
            }
        }
    }
}

/** The binding metadata of the generated test (native binding only). */
function php_binding_model(array $scenario): array
{
    $nativeBinding = null;
    foreach ($scenario['bindings'] ?? [] as $binding) {
        if (is_array($binding) && ($binding['backend'] ?? null) === 'native') {
            $nativeBinding = $binding;
            break;
        }
    }
    if ($nativeBinding === null) {
        return ['mode' => 'generated', 'backend' => 'none', 'test' => null, 'capabilityDigest' => null];
    }
    return [
        'mode' => is_string($nativeBinding['mode'] ?? null) ? $nativeBinding['mode'] : 'generated',
        'backend' => 'native',
        'test' => is_string($nativeBinding['test'] ?? null) ? $nativeBinding['test'] : null,
        'capabilityDigest' => is_string($nativeBinding['capabilityDigest'] ?? null)
            ? $nativeBinding['capabilityDigest'] : null,
    ];
}

// ---------------------------------------------------------------------------
// Step mapping: given / when / then, each port-joined.
// ---------------------------------------------------------------------------

function php_map_given(array $given, array $portSurface): array
{
    $surfaceOf = [
        'state' => 'state',
        'fixture' => 'fixtures',
        'actor' => 'actor',
        'clock' => 'clock',
        'id_source' => 'ids',
    ];
    return array_map(static function (array $step) use ($surfaceOf, $portSurface): array {
        $precondition = is_array($step['precondition'] ?? null) ? $step['precondition'] : [];
        $kind = $precondition['kind'] ?? null;
        $mapped = [
            'stepId' => $step['stepId'],
            'kind' => in_array($kind, PHP_PRECONDITION_KINDS, true) ? $kind : 'unknown',
            'port' => null,
            'unsupported' => null,
        ];
        $surface = $surfaceOf[$kind] ?? null;
        if ($surface === null) {
            $mapped['unsupported'] = [
                'capability' => 'scenario.precondition.' . ($kind ?? 'unknown'),
                'reason' => 'precondition-kind-unknown',
            ];
            return $mapped;
        }
        if (!$portSurface[$surface]) {
            $mapped['unsupported'] = [
                'capability' => 'testing.' . ($surface === 'ids' ? 'ids' : $surface),
                'reason' => 'port-surface-absent',
                'detail' => $surface,
            ];
            return $mapped;
        }
        $mapped['port'] = $surface;
        $mapped['payload'] = php_precondition_payload($precondition);
        // Typed leaves propagate as unsupported rows (review R-3), never
        // as mid-render crashes.
        if ($kind === 'state') {
            $problem = php_state_leaf_problem($mapped['payload']);
            if ($problem !== null) {
                $mapped['unsupported'] = [
                    'capability' => 'scenario.value',
                    'reason' => $problem['reason'],
                    'detail' => php_bound_token($problem['field']),
                ];
            }
        }
        return $mapped;
    }, $given);
}

/** The first unrenderable leaf of one mapped state precondition. */
function php_state_leaf_problem(array $payload): ?array
{
    foreach ($payload['selector'] ?? [] as $term) {
        $problem = php_check_leaf($term['equals'] ?? null, 0);
        if ($problem !== null) {
            return ['reason' => $problem, 'field' => $term['field'] ?? null];
        }
    }
    foreach ($payload['fields'] ?? [] as $entry) {
        $problem = php_check_leaf($entry[1] ?? null, 0);
        if ($problem !== null) {
            return ['reason' => $problem, 'field' => $entry[0] ?? null];
        }
    }
    return null;
}

/** The first unrenderable leaf of one mapped entity_state assertion. */
function php_entity_state_leaf_problem(array $payload): ?array
{
    foreach ($payload['where'] ?? [] as $term) {
        $problem = php_check_leaf($term['equals'] ?? null, 0);
        if ($problem !== null) {
            return ['reason' => $problem, 'field' => $term['field'] ?? null];
        }
    }
    foreach ($payload['fields'] ?? [] as $field => $expectation) {
        if (is_array($expectation) && array_key_exists('match', $expectation)) {
            continue; // A match-kind expectation is a kind token, never a leaf.
        }
        $problem = php_check_leaf(is_array($expectation) && array_key_exists('value', $expectation)
            ? $expectation['value'] : $expectation, 0);
        if ($problem !== null) {
            return ['reason' => $problem, 'field' => $field];
        }
    }
    return null;
}

function php_precondition_payload(array $precondition): array
{
    switch ($precondition['kind'] ?? null) {
        case 'state':
            $fields = [];
            foreach ($precondition['fields'] ?? [] as $field => $leaf) {
                $fields[] = [$field, $leaf];
            }
            return [
                'entity' => $precondition['entity'] ?? null,
                'selector' => array_map(static fn (array $term): array => [
                    'field' => $term['field'] ?? null,
                    'equals' => $term['equals'] ?? null,
                ], $precondition['selector'] ?? []),
                'fields' => $fields,
            ];
        case 'fixture':
            return [
                'fixture' => $precondition['fixture'] ?? null,
                'version' => $precondition['version'] ?? null,
                'capabilities' => $precondition['capabilities'] ?? [],
            ];
        case 'actor':
            return [
                'actor' => $precondition['actor'] ?? null,
                'scope' => $precondition['scope'] ?? null,
            ];
        case 'clock':
            return ['at' => $precondition['at']['value'] ?? null];
        case 'id_source':
            return [
                'seed' => $precondition['seed'] ?? null,
                'algorithm' => $precondition['algorithm'] ?? null,
            ];
        default:
            return [];
    }
}

function php_map_when(array $when, array &$context, array $portSurface): array
{
    return array_map(static function (array $step) use (&$context, $portSurface): array {
        $action = is_array($step['action'] ?? null) ? $step['action'] : [];
        $mapped = [
            'stepId' => $step['stepId'],
            'kind' => 'invoke',
            'operation' => null,
            'input' => [],
            'ctx' => [],
            'replay' => null,
            'unsupported' => null,
        ];
        if (($action['kind'] ?? null) !== 'invoke') {
            $mapped['unsupported'] = ['capability' => 'scenario.action', 'reason' => 'action-kind-unknown'];
            return $mapped;
        }
        $operationId = $action['operation'] ?? null;
        if (!is_string($operationId)) {
            $context['findings'][] = [
                'code' => 'scenario.operation-unresolved',
                'symbol' => $context['scenarioId'],
                'detail' => php_bound_token((string) $operationId),
            ];
            $mapped['unsupported'] = [
                'capability' => 'scenario.operation',
                'reason' => 'operation-unresolved',
                'detail' => php_bound_token((string) $operationId),
            ];
            return $mapped;
        }
        $operation = php_resolve_operation($context['operationIndex'], $operationId);
        if ($operation === null) {
            $context['findings'][] = [
                'code' => 'scenario.operation-unresolved',
                'symbol' => $context['scenarioId'],
                'detail' => php_bound_token($operationId),
            ];
            $mapped['unsupported'] = [
                'capability' => 'scenario.operation',
                'reason' => 'operation-unresolved',
                'detail' => php_bound_token($operationId),
            ];
            return $mapped;
        }
        $mapped['operation'] = $operation;
        $mapped['input'] = [];
        foreach ($action['input'] ?? [] as $field => $leaf) {
            $mapped['input'][] = [
                'field' => $field,
                'leaf' => $leaf,
                'leafProblem' => php_check_leaf($leaf, 0),
            ];
        }
        // Review F-9: a `when`-input leaf outside the closed typed set is
        // unsupported, never a crash.
        foreach ($mapped['input'] as $entry) {
            if ($entry['leafProblem'] !== null) {
                $mapped['unsupported'] = [
                    'capability' => 'scenario.value',
                    'reason' => $entry['leafProblem'],
                    'detail' => php_bound_token($entry['field']),
                ];
                break;
            }
        }
        if (array_key_exists('actor', $action)) {
            $mapped['ctx']['actor'] = $action['actor'];
        }
        if (array_key_exists('clock', $action)) {
            $mapped['ctx']['clock'] = $action['clock'];
        }
        if (array_key_exists('idempotencyKey', $action)) {
            $mapped['ctx']['idempotencyKey'] = $action['idempotencyKey'];
            $problem = php_check_leaf($action['idempotencyKey'], 0);
            if ($problem !== null) {
                $mapped['unsupported'] = ['capability' => 'scenario.value', 'reason' => $problem];
            }
        }
        if (!$portSurface['invoke']) {
            $mapped['unsupported'] = [
                'capability' => 'testing.fixtures',
                'reason' => 'port-surface-absent',
                'detail' => 'invoke',
            ];
        }
        if (is_array($step['replay'] ?? null)) {
            $mapped['replay'] = [
                'of' => $step['replay']['of'] ?? null,
                'expect' => $step['replay']['expect'] ?? null,
            ];
        }
        return $mapped;
    }, $when);
}

function php_map_then(array $then, array &$context, array $portSurface): array
{
    $surfaceOf = [
        'entity_state' => 'state',
        'emitted' => 'emissions',
        'forbidden_effect' => 'effects',
        'authorization' => 'authorize',
        'contract_match' => 'contractCheck',
        'deterministic_fixture' => 'fixtureDigest',
    ];
    return array_map(static function (array $step) use (&$context, $surfaceOf, $portSurface): array {
        $assertion = is_array($step['assertion'] ?? null) ? $step['assertion'] : [];
        $kind = $assertion['kind'] ?? null;
        $mapped = [
            'stepId' => $step['stepId'],
            'observes' => $step['observes'] ?? null,
            'kind' => in_array($kind, PHP_ASSERTION_KINDS, true) ? $kind : 'unknown',
            'port' => null,
            'unsupported' => null,
            'payload' => [],
        ];
        if (!in_array($kind, PHP_ASSERTION_KINDS, true)) {
            $mapped['unsupported'] = ['capability' => 'scenario.assertion', 'reason' => 'assertion-kind-unknown'];
            return $mapped;
        }
        if ($kind === 'unsupported') {
            // The explicit unsupported expectation: always a recorded
            // unsupported row carrying the capability ref — never a pass.
            $mapped['unsupported'] = [
                'capability' => $assertion['capability'] ?? 'scenario.capability',
                'reason' => 'declared-unsupported',
                'detail' => array_key_exists('note', $assertion)
                    ? php_bound_token((string) $assertion['note']) : null,
            ];
            return $mapped;
        }
        if ($kind === 'result') {
            $mapped['payload']['valueType'] = $assertion['valueType'] ?? null;
            if (array_key_exists('value', $assertion)) {
                $mapped['payload']['value'] = $assertion['value'];
                $problem = php_check_leaf($assertion['value'], 0);
                if ($problem !== null) {
                    $mapped['unsupported'] = ['capability' => 'scenario.value', 'reason' => $problem];
                }
            }
            return $mapped;
        }
        if ($kind === 'error') {
            $mapped['payload']['error'] = $assertion['error'] ?? null;
            $mapped['payload']['payload'] = [];
            foreach ($assertion['payload'] ?? [] as $field => $leaf) {
                $mapped['payload']['payload'][] = [
                    'field' => $field,
                    'leaf' => $leaf,
                    'leafProblem' => php_check_leaf($leaf, 0),
                ];
            }
            $mapped['payload']['contract'] = $assertion['contract'] ?? null;
            if ($mapped['payload']['contract'] !== null && !$portSurface['contractCheck']) {
                // The contract half needs the port contract check; the
                // typed error identity and public-field subset stay supported.
                $mapped['unsupported'] = [
                    'capability' => 'scenario.contract-check',
                    'reason' => 'port-surface-absent',
                    'detail' => 'contractCheck',
                ];
            }
            return $mapped;
        }
        if ($kind === 'idempotency') {
            // The weaker semantic-equivalence relation has no evaluator in
            // v1; an explicit unsupported row, never a proxy.
            if (($assertion['equivalence'] ?? null) === 'equivalent') {
                $mapped['unsupported'] = [
                    'capability' => 'scenario.equivalence-equivalent',
                    'reason' => 'equivalence-unimplemented',
                    'detail' => 'equivalent',
                ];
                return $mapped;
            }
            $mapped['payload']['replay'] = $assertion['replay'] ?? null;
            $mapped['payload']['equivalence'] = $assertion['equivalence'] ?? null;
            $mapped['payload']['duplicates'] = $assertion['duplicates'] ?? null;
            return $mapped;
        }
        $surface = $surfaceOf[$kind] ?? null;
        if ($surface !== null && !$portSurface[$surface]) {
            $mapped['unsupported'] = [
                'capability' => 'testing.' . $surface,
                'reason' => 'port-surface-absent',
                'detail' => $surface,
            ];
            return $mapped;
        }
        if ($kind === 'forbidden_effect' && ($assertion['scope'] ?? null) === 'resource') {
            // No resource ledger surface exists on the closed port contract.
            $mapped['unsupported'] = [
                'capability' => 'scenario.forbidden-scope-resource',
                'reason' => 'scope-unimplemented',
                'detail' => 'resource',
            ];
            return $mapped;
        }
        if ($kind === 'entity_state') {
            // A matcher outside the closed vocabulary is unsupported,
            // never silently weakened.
            $known = ['datetime', 'uuid', 'uri', 'decimal', 'non-null'];
            $unknown = [];
            foreach ($assertion['fields'] ?? [] as $field => $expectation) {
                if (is_array($expectation) && array_key_exists('match', $expectation)
                    && !in_array($expectation['match'], $known, true)) {
                    $unknown[] = $field . ':' . $expectation['match'];
                }
            }
            if ($unknown !== []) {
                $mapped['unsupported'] = [
                    'capability' => 'scenario.match-kind',
                    'reason' => 'match-kind-unimplemented',
                    'detail' => php_bound_token(implode(',', $unknown)),
                ];
                return $mapped;
            }
        }
        $mapped['port'] = $surface ?? null;
        $mapped['payload'] = $assertion;
        unset($mapped['payload']['kind']);
        if ($kind === 'entity_state') {
            $problem = php_entity_state_leaf_problem($mapped['payload']);
            if ($problem !== null) {
                $mapped['unsupported'] = [
                    'capability' => 'scenario.value',
                    'reason' => $problem['reason'],
                    'detail' => php_bound_token($problem['field']),
                ];
            }
        }
        foreach ($mapped['payload'] as $value) {
            if (is_array($value) && !array_is_list($value)
                && (isset($value['$ref']) || isset($value['type']))) {
                $problem = php_check_leaf($value, 0);
                if ($problem !== null) {
                    $mapped['unsupported'] = ['capability' => 'scenario.value', 'reason' => $problem];
                }
            }
        }
        return $mapped;
    }, $then);
}

/** Bounded, control-cleaned detail token (no raw attacker text). */
function php_bound_token(mixed $text): string
{
    $value = (string) ($text ?? 'unknown');
    $value = preg_replace('/[^a-zA-Z0-9._:\\/-]+/', '?', $value) ?? '?';
    return substr($value, 0, 128);
}

/** The canonical AST digest input: the mapper model in canonical JSON. */
function php_ast_digest_input(array $model): string
{
    return php_canonical_json($model);
}

// ---------------------------------------------------------------------------
// The checked-binding join (issue #56, plan S3) — a structural port of
// `joinCheckedBindings` in the Node scenario compiler.
// ---------------------------------------------------------------------------

const PHP_BINDING_MISSING = 'scenario.binding-missing';
const PHP_BINDING_AMBIGUOUS = 'scenario.binding-ambiguous';
const PHP_BINDING_MISMATCH = 'scenario.binding-mismatch';

/**
 * Join every native `checked` binding against the observed index's
 * `test_bindings` records — the scan pipeline's view of which native
 * tests claim which `lekalo:<id>` scenario identities. The join is
 * pure and read-only: a missing, ambiguous, or stale binding is a
 * typed finding, never a silent pass and never a rewrite.
 *
 * `indexDocument` is the parsed observed index (`null` when absent —
 * legal absence: the join simply has nothing to say). Returns one
 * finding per violated binding, ordered by the document's binding
 * order.
 */
function php_join_checked_bindings(mixed $scenarioDocument, mixed $indexDocument): array
{
    $findings = [];
    if (!is_array($indexDocument) || !is_array($indexDocument['test_bindings'] ?? null)) {
        return $findings;
    }
    $claims = [];
    foreach ($indexDocument['test_bindings'] as $record) {
        if (!is_array($record)) {
            continue;
        }
        $ids = php_claimed_ids($record['id'] ?? null);
        if ($ids === []) {
            continue;
        }
        $claims[] = [
            'ids' => $ids,
            'symbol' => is_string($record['symbol'] ?? null) ? $record['symbol'] : null,
            'fingerprint' => is_string($record['fingerprint'] ?? null) ? $record['fingerprint'] : null,
        ];
    }
    foreach (is_array($scenarioDocument) ? ($scenarioDocument['bindings'] ?? []) : [] as $binding) {
        if (!is_array($binding)) {
            continue;
        }
        if (($binding['backend'] ?? null) !== 'native' || ($binding['mode'] ?? null) !== 'checked') {
            continue;
        }
        if (!is_string($binding['test'] ?? null)) {
            continue;
        }
        $testId = $binding['test'];
        $claiming = [];
        foreach ($claims as $claim) {
            if (in_array($testId, $claim['ids'], true)) {
                $claiming[] = $claim;
            }
        }
        if (count($claiming) === 0) {
            $findings[] = ['code' => PHP_BINDING_MISSING, 'symbol' => $testId, 'detail' => 'no-scanned-test'];
            continue;
        }
        if (count($claiming) > 1) {
            $findings[] = [
                'code' => PHP_BINDING_AMBIGUOUS,
                'symbol' => $testId,
                'detail' => 'claimed-by-' . count($claiming) . '-tests',
            ];
            continue;
        }
        // One native test file may legitimately cover several scenarios
        // (one shared fixture setup, one harness), so a record whose
        // claimed set CONTAINS the bound id joins cleanly; a declared
        // evidence digest that disagrees with the scanned fingerprint
        // means the test changed under the binding — stale evidence.
        $record = $claiming[0];
        if (is_string($binding['evidenceDigest'] ?? null) && $binding['evidenceDigest'] !== ''
            && $record['fingerprint'] !== null
            && $binding['evidenceDigest'] !== $record['fingerprint']) {
            $findings[] = [
                'code' => PHP_BINDING_MISMATCH,
                'symbol' => $testId,
                'detail' => 'stale-evidence-digest',
            ];
        }
    }
    return $findings;
}

/**
 * The claimed scenario ids of one observed test-binding id: the core
 * spells them `<test-path>#lekalo:<id>[,lekalo:<id>…]`; a bare
 * `lekalo:<id>` (no path half) still joins.
 */
function php_claimed_ids(mixed $id): array
{
    if (!is_string($id)) {
        return [];
    }
    $hash = strrpos($id, '#');
    $name = $hash === false ? $id : substr($id, $hash + 1);
    $claimed = [];
    foreach (explode(',', $name) as $part) {
        if (str_starts_with($part, 'lekalo:')) {
            $value = substr($part, strlen('lekalo:'));
            if ($value !== '') {
                $claimed[] = $value;
            }
        }
    }
    return $claimed;
}

// ----- bundled compiler module: scenario-emit.php -----


/**
 * Deterministic PHP emitter for the scenario-test compiler (issue #56,
 * S2). A structural port of the Node emitter
 * (`adapters/node-typescript/src/scenario-emit.mjs`): input is the pure
 * test model of `scenario-map.php`; output is one strict PHP test file
 * per scenario plus the shared `ScenarioTestKit.php`, the run-record
 * reporter `ScenarioReporter.php`, and the port shim `Port.php`, each
 * test paired with one canonical `.test.map.json` sidecar.
 *
 * Byte stability is the contract: a fixed header comment (adapter id /
 * version, contract identities, the `sha256:` digest of the exact
 * scenario document bytes — no timestamps, no host paths, no host
 * data), deterministic scenario/step ordering, 4-space indent, LF
 * endings, no trailing whitespace, exactly one final newline. String
 * literals are emitted via `php_emit_value`, whose escaping is
 * `var_export`-free single-quote encoding, so no interpolation or code
 * injection survives into a generated test.
 *
 * Evidence honesty mirrors the Node emitter: every assertion block
 * records exactly one outcome row (`pass | fail | unsupported |
 * infrastructure | degraded`), unsupported rows can never become
 * passes, and the reporter persists the rows into the durable run
 * record.
 */

const PHP_EMITTER_ADAPTER_ID = 'lekalo-target-php-laravel';

/** The sidecar micro-contract token of the scenario test maps. */
const PHP_MAP_CONTRACT = 'lekalo/scenario-test-map/v0.4.0';

/** The runner version reported when a scenario declares no explicit pin. */
const PHP_RUNNER_VERSION = 'bundled-toolchain';

/** The run-record contract family the reporter writes. */
const PHP_RUN_RECORD_SCHEMA_VERSION = 'lekalo/scenario-run/v0.4.0';
const PHP_RUN_RECORD_IDENTITY = 'dev.lekalo.scenario-run@0.4.0';

/** The run-record ingest home (an adjudicated `.lekalo/import` home). */
const PHP_RUN_RECORD_DIR = '.lekalo/import/scenario-runs';

/**
 * The toolchain custody record (issue #56, plan S1): one durable
 * document per suite run recording the OBSERVED runtime facts — PHP
 * version, the resolved Laratesto/Testo/Laravel package versions, and
 * the exact `composer.lock` digest — separate from the closed
 * run-record shape so custody never overloads the single `runner`
 * field.
 */
const PHP_TOOLCHAIN_DIR = '.lekalo/import/toolchain';
const PHP_TOOLCHAIN_SCHEMA_VERSION = 'lekalo/scenario-toolchain/v0.1.0';
const PHP_TOOLCHAIN_IDENTITY = 'dev.lekalo.scenario-toolchain@0.1.0';

/**
 * The conformance-relevant Composer packages the custody record
 * probes. The list is fixed emission data — never derived from the
 * project's composer.json at compile time, and 'not-installed' is an
 * honest row, never an omission.
 */
const PHP_TOOLCHAIN_PACKAGES = ['ichinya/laratesto', 'laravel/framework', 'testo/testo'];

/**
 * The user-owned scaffold home (issue #56, plan S3): a `scaffolded`
 * binding emits its test here exactly once — every later generation
 * keeps the user's bytes — so it must sit outside the managed
 * generated home. The segments stay lowercase because the write plan
 * travels the logical-path grammar (uppercase is wire-illegal).
 */
const PHP_SCAFFOLD_TESTS_DIR = 'tests/lekalo/scenario-tests';

/** Reserved emitted module names; a scenario module may never collide. */
const PHP_RESERVED_MODULES = ['testkit', 'port', 'reporter', 'ScenarioTestKit', 'Port', 'ScenarioReporter'];

/** The generated support files shared by every scenario test. */
const PHP_SUPPORT_FILES = [
    'scenario-test-kit.php',
    'scenario-reporter.php',
    'port.php',
];

/**
 * The semantic native-test id of one scenario on one target: the target
 * token is part of the identity, so a Node test and a Laravel test for
 * the same scenario never collide into one trace node.
 */
function php_native_test_id(string $scenarioId): string
{
    return 'php-laravel:' . $scenarioId;
}

/** The stable PHP FQN of one scenario's generated test class. */
function php_test_class_fqn(string $scenarioId): string
{
    return 'Lekalo\\Generated\\ScenarioTests\\' . php_module_of($scenarioId) . '\\'
        . php_class_of($scenarioId);
}

/**
 * The emission module of one scenario: the id prefix before the first
 * dot (the module namespace segment).
 */
function php_module_of(string $scenarioId): string
{
    $cut = strpos($scenarioId, '.');
    return $cut === false || $cut === 0 ? $scenarioId : substr($scenarioId, 0, $cut);
}

/** The PSR-4-safe class identifier of one scenario id: always ends
 * with `Test` (the case-suffix the naming convention locates) and the
 * emitted file spelling `<id>.test.php` ends with the file suffix
 * `Test.php` is checked against — the class name is what matters, so
 * the generated class carries the suffix.
 */
function php_class_of(string $scenarioId): string
{
    $sanitized = preg_replace('/[^a-zA-Z0-9]/', '_', $scenarioId);
    $parts = array_map(static fn (string $part): string => ucfirst($part), explode('_', $sanitized));
    // The Laratesto naming convention discovers `*Test.php` files whose
    // class name also ends in `Test`, so the suffix is part of the
    // stable class mapping.
    return implode('', $parts) . 'Test';
}

/** The safe PHP identifier of one scenario or step id (snake_case use). */
function php_identifier_of(string $id): string
{
    $sanitized = preg_replace('/[^a-zA-Z0-9_]/', '_', $id);
    // Extension-free leading-digit probe: strspn ships with ext/standard,
    // so the check survives `php -n` runtimes where ctype is absent.
    return strspn($sanitized, '0123456789') > 0 ? '_' . $sanitized : $sanitized;
}

/**
 * One comment-safe single-line projection of free wire text: every line
 * terminator and control character collapses, so a core-valid `summary`
 * can never close a generated comment and inject live code into the
 * emitted test.
 */
function php_comment_safe(mixed $text): string
{
    $value = (string) ($text ?? '');
    $value = preg_replace('/\r\n|[\r\n\x{0085}\x{2028}\x{2029}]|\p{Cc}/u', ' ', $value) ?? '';
    $value = preg_replace('/\s+/', ' ', $value) ?? '';
    $value = trim($value);
    // Byte-exact truncation: the scenario byte-range maps account bytes,
    // so the cap must count bytes (mb_* would count characters and
    // desynchronize the map); it also keeps php -n runtimes safe.
    $value = substr($value, 0, 200);
    // A one-line comment also ends at the mid-line close-tag pair (the
    // question-mark/greater-than sequence a hostile summary can carry):
    // lints clean, dumps the file tail as output, and the test class
    // never gets defined. Break the pair — a one-line comment has no
    // other close sequence — so no projection can re-open PHP mode.
    return str_replace('?>', '? >', $value);
}

/**
 * Emit every generated file of one mapped scenario document.
 *
 * `input` is `{models, inputDigest, adapterVersion, portModulePath,
 * startedBy}`. Returns sorted `{path, text, map}` records; `map` is
 * non-null only on sidecars. A checked binding emits nothing for its
 * scenario (review F-4: the checked identity belongs exclusively to the
 * existing native test).
 */
function php_emit_scenario_tests(array $input): array
{
    $context = [
        'inputDigest' => $input['inputDigest'],
        'adapterVersion' => $input['adapterVersion'],
        'portModulePath' => $input['portModulePath'],
        'portClass' => $input['portClass'] ?? '',
        'startedBy' => $input['startedBy'] ?? 'lekalo-scenario-harness',
    ];
    if ($context['portClass'] === '' && $context['portModulePath'] !== '') {
        // A declared port always carries both its logical path and its
        // class; only the declaration-absent compile (every feature an
        // explicit unsupported row, the shim never invoked) emits with
        // an empty class binding.
        throw new LogicException('scenario emit without a validated port class');
    }
    $files = [
        php_file(PHP_SCENARIO_TESTS_DIR . '/scenario-test-kit.php', php_testkit_text($context), null),
        php_file(PHP_SCENARIO_TESTS_DIR . '/scenario-reporter.php', php_reporter_text($context), null),
        php_file(PHP_SCENARIO_TESTS_DIR . '/port.php', php_port_text($context), null),
    ];
    $models = $input['models'];
    usort($models, static fn (array $left, array $right): int => strcmp($left['id'], $right['id']));
    foreach ($models as $model) {
        $mode = $model['binding']['mode'];
        if ($mode === 'checked') {
            // A checked binding declares that an EXISTING native test
            // owns the scenario identity: nothing is generated for it.
            continue;
        }
        $module = php_module_of($model['id']);
        if (in_array($module, PHP_RESERVED_MODULES, true)) {
            throw new LogicException('scenario module collides with a reserved emitted file: ' . $module);
        }
        $scaffolded = $mode === 'scaffolded';
        // A scaffolded test is user-owned: it is emitted once into the
        // scaffold home (`frozen` — the kernel plans its write only
        // when absent) and its map sidecar travels beside it as the
        // scaffold marker the custody rules key on.
        $dir = ($scaffolded ? PHP_SCAFFOLD_TESTS_DIR : PHP_SCENARIO_TESTS_DIR) . '/' . $module;
        $testFile = php_emit_test($model, $context, $scaffolded);
        $mapPath = $dir . '/' . $model['id'] . '.test.map.json';
        $test = php_file($dir . '/' . $model['id'] . '.test.php', $testFile['text'], null);
        if ($scaffolded) {
            $test['frozen'] = true;
            $test['marker'] = $mapPath;
        }
        $files[] = $test;
        $files[] = php_file($mapPath, php_canonical_json($testFile['map']) . "\n", $testFile['map']);
    }
    usort($files, static fn (array $left, array $right): int => strcmp($left['path'], $right['path']));
    return $files;
}

function php_file(string $path, string $text, ?array $map): array
{
    return ['path' => $path, 'text' => $text, 'map' => $map];
}

// ---------------------------------------------------------------------------
// Shared emitted support files.
// ---------------------------------------------------------------------------

function php_doc_header(array $context): string
{
    // The open tag leads every emitted file: without it PHP would parse
    // the whole file as inline HTML and the class would never exist.
    return "<?php\n\n// Generated by " . PHP_EMITTER_ADAPTER_ID . '@' . $context['adapterVersion']
        . ' (scenario-test-compiler, issue #56).' . "\n"
        . '// From ' . PHP_SCENARIO_IDENTITY . ' input ' . $context['inputDigest'] . '.'
        . ' Do not edit: regenerate with `lekalo generate`.';
}

function php_testkit_text(array $context): string
{
    $header = php_doc_header($context);
    return <<<PHP
$header
// The shared runner-neutral helpers; content depends only on the
// adapter version, so this file is itself a determinism probe.
// Generated file — do not edit.

declare(strict_types=1);

namespace Lekalo\\Generated\\ScenarioTests;

use Testo\\Assert;

/**
 * Canonical typed equality over the closed Scenario IR value domain:
 * dates, datetimes, uuids, uris, and decimals compare exactly as their
 * canonical strings; integers compare numerically; objects compare
 * field-by-field in any key order.
 */
final class ScenarioTestKit
{
    /**
     * One bounded, control-cleaned failure detail: the run record
     * carries no absolute paths, no host data, and never more than one
     * short line.
     */
    public static function boundedDetail(mixed \$value): string
    {
        \$text = \$value === null ? 'unknown' : (string) \$value;
        \$text = preg_replace('/[^a-zA-Z0-9._:\\/() -]+/', '?', \$text) ?? '?';
        \$text = trim(\$text);
        return substr(\$text, 0, 200);
    }

    /** Canonical typed equality (see class docblock). */
    public static function typedEqual(mixed \$actual, mixed \$expected): bool
    {
        if (\$actual === \$expected) {
            return true;
        }
        if (is_object(\$actual) || is_object(\$expected)) {
            if (!is_object(\$actual) || !is_object(\$expected)) {
                return false;
            }
            if (get_class(\$actual) !== get_class(\$expected)) {
                return false;
            }
            return self::typedEqual((array) \$actual, (array) \$expected);
        }
        if (is_array(\$actual) || is_array(\$expected)) {
            if (!is_array(\$actual) || !is_array(\$expected)) {
                return false;
            }
            if (array_is_list(\$actual) !== array_is_list(\$expected)) {
                return false;
            }
            if (array_is_list(\$actual)) {
                if (count(\$actual) !== count(\$expected)) {
                    return false;
                }
                foreach (\$actual as \$index => \$item) {
                    if (!self::typedEqual(\$item, \$expected[\$index])) {
                        return false;
                    }
                }
                return true;
            }
            \$leftKeys = array_keys(\$actual);
            sort(\$leftKeys, SORT_STRING);
            \$rightKeys = array_keys(\$expected);
            sort(\$rightKeys, SORT_STRING);
            if (\$leftKeys !== \$rightKeys) {
                return false;
            }
            foreach (\$leftKeys as \$key) {
                if (!self::typedEqual(\$actual[\$key], \$expected[\$key])) {
                    return false;
                }
            }
            return true;
        }
        return false;
    }

    /**
     * The public subset of one error object: the id plus the declared
     * payload fields only — a generated test never asserts private
     * error internals.
     */
    public static function errorFieldsMatch(mixed \$error, array \$fields): bool
    {
        if (!is_array(\$error)) {
            return false;
        }
        foreach (\$fields as \$key => \$expected) {
            \$carried = \$error['fields'][\$key] ?? null;
            if (!self::typedEqual(\$carried, \$expected)) {
                return false;
            }
        }
        return true;
    }

    // -----------------------------------------------------------------
    // Canonical-form matchers: exact ports of the core grammar
    // functions in scenario/value.rs. A stored state field that
    // violates the canonical contract must fail the generated matcher —
    // over-accepting approximations are false passes.
    // -----------------------------------------------------------------

    /** Real Gregorian month lengths, leap years included (value.rs). */
    private static function daysInMonth(int \$year, int \$month): int
    {
        if (in_array(\$month, [1, 3, 5, 7, 8, 10, 12], true)) {
            return 31;
        }
        if (in_array(\$month, [4, 6, 9, 11], true)) {
            return 30;
        }
        return ((\$year % 4 === 0 && \$year % 100 !== 0) || \$year % 400 === 0) ? 29 : 28;
    }

    /** The canonical calendar date with real month and day values. */
    private static function canonicalDate(string \$text): bool
    {
        if (preg_match('/^[0-9]{4}-[0-9]{2}-[0-9]{2}$/', \$text) !== 1) {
            return false;
        }
        \$year = (int) substr(\$text, 0, 4);
        \$month = (int) substr(\$text, 5, 2);
        \$day = (int) substr(\$text, 8, 2);
        if (\$year < 1 || \$year > 9999 || \$month < 1 || \$month > 12) {
            return false;
        }
        return \$day >= 1 && \$day <= self::daysInMonth(\$year, \$month);
    }

    /** The canonical decimal spelling (value.rs canonical_decimal). */
    public static function canonicalDecimal(mixed \$text): bool
    {
        \$value = (string) \$text;
        if (\$value === '-0') {
            return false;
        }
        \$negative = str_starts_with(\$value, '-');
        \$rest = \$negative ? substr(\$value, 1) : \$value;
        \$dot = strpos(\$rest, '.');
        \$integral = \$dot === false ? \$rest : substr(\$rest, 0, \$dot);
        \$fractional = \$dot === false ? null : substr(\$rest, \$dot + 1);
        if (preg_match('/^[0-9]+$/', \$integral) !== 1) {
            return false;
        }
        if (strlen(\$integral) > 1 && str_starts_with(\$integral, '0')) {
            return false;
        }
        if (\$integral === '0' && \$negative) {
            return false;
        }
        if (\$fractional === null) {
            return true;
        }
        return preg_match('/^[0-9]+$/', \$fractional) === 1 && !str_ends_with(\$fractional, '0');
    }

    /** The canonical UTC datetime (value.rs canonical_datetime). */
    public static function canonicalDatetime(mixed \$text): bool
    {
        \$value = (string) \$text;
        if (strlen(\$value) < 20 || !str_ends_with(\$value, 'Z')) {
            return false;
        }
        if (!self::canonicalDate(substr(\$value, 0, 10))) {
            return false;
        }
        if (\$value[10] !== 'T') {
            return false;
        }
        \$time = substr(\$value, 11, strlen(\$value) - 12);
        \$dot = strpos(\$time, '.');
        \$clock = \$dot === false ? \$time : substr(\$time, 0, \$dot);
        \$fraction = \$dot === false ? null : substr(\$time, \$dot + 1);
        \$parts = explode(':', \$clock);
        if (count(\$parts) !== 3) {
            return false;
        }
        foreach (\$parts as \$part) {
            if (strlen(\$part) !== 2 || preg_match('/^[0-9]+$/', \$part) !== 1) {
                return false;
            }
        }
        \$hour = (int) \$parts[0];
        \$minute = (int) \$parts[1];
        \$second = (int) \$parts[2];
        if (\$hour > 23 || \$minute > 59 || \$second > 59) {
            return false;
        }
        if (\$fraction === null) {
            return true;
        }
        \$length = strlen(\$fraction);
        return \$length >= 1 && \$length <= 9 && preg_match('/^[0-9]+$/', \$fraction) === 1;
    }

    /** The canonical URI (value.rs canonical_uri). */
    public static function canonicalUri(mixed \$text): bool
    {
        \$value = (string) \$text;
        \$bytes = strlen(\$value);
        if (\$bytes < 8 || \$bytes > 2048) {
            return false;
        }
        \$marker = strpos(\$value, '://');
        if (\$marker === false) {
            return false;
        }
        \$scheme = substr(\$value, 0, \$marker);
        \$rest = substr(\$value, \$marker + 3);
        if (\$scheme === '' || !preg_match('/^[a-z]/', \$scheme)) {
            return false;
        }
        if (preg_match('/^[a-z0-9+.-]*$/', substr(\$scheme, 1)) !== 1) {
            return false;
        }
        if (\$rest === '') {
            return false;
        }
        if (preg_match('/[<>"{}|\\\\^` ]|[\\x00-\\x1f\\x7f-\\x9f]/', \$value) === 1) {
            return false;
        }
        \$authorityMatch = strpbrk(\$rest, '/?#');
        \$authorityEnd = \$authorityMatch === false ? strlen(\$rest) : strpos(\$rest, \$authorityMatch[0]);
        return strpos(substr(\$rest, 0, \$authorityEnd), '@') === false;
    }
}

PHP;
}

function php_reporter_text(array $context): string
{
    $header = php_doc_header($context);
    $schemaVersion = PHP_RUN_RECORD_SCHEMA_VERSION;
    $identity = PHP_RUN_RECORD_IDENTITY;
    $ingestDir = PHP_RUN_RECORD_DIR;
    $toolchainDir = PHP_TOOLCHAIN_DIR;
    $toolchainSchema = PHP_TOOLCHAIN_SCHEMA_VERSION;
    $toolchainIdentity = PHP_TOOLCHAIN_IDENTITY;
    $adapterId = PHP_EMITTER_ADAPTER_ID;
    $adapterVersion = ADAPTER_VERSION;
    $probedPackages = implode(', ', array_map(
        static fn (string $package): string => "'" . $package . "'",
        PHP_TOOLCHAIN_PACKAGES,
    ));
    return <<<PHP
$header
// The durable run-record writer: one $schemaVersion document per
// scenario run, written into the adjudicated ingest home $ingestDir/,
// plus one toolchain custody record in $toolchainDir/ recording the
// observed runtime facts (PHP, the resolved Laratesto/Testo/Laravel
// versions, the composer.lock digest) — the closed run-record shape
// never carries toolchain data.
// Generated file — do not edit.

declare(strict_types=1);

namespace Lekalo\\Generated\\ScenarioTests;

/**
 * One run recorder for one scenario test. Records exactly one bounded
 * outcome row per executed assertion and flushes the closed
// run-record document into the ingest home. The test fingerprint is
 * computed over the exact bytes of the importing test file at flush
 * time; the project root is derived from this file's own fixed
 * location under the generated root, never from the cwd.
 */
final class ScenarioReporter
{
    private const GENERATED_ROOT_DEPTH = 4;

    private array \$assertions = [];

    /** @param array<string, mixed> \$spec */
    public function __construct(
        private readonly array \$spec,
        private readonly string \$testFile,
    ) {}

    /**
     * Record exactly one assertion outcome row. Outcomes are closed:
     * pass | fail | unsupported | infrastructure | degraded. An
     * unsupported row can never become a pass.
     */
    public function record(array \$row): void
    {
        \$outcome = (string) \$row['outcome'];
        if (!in_array(\$outcome, ['pass', 'fail', 'unsupported', 'infrastructure', 'degraded'], true)) {
            throw new LogicException('closed outcome vocabulary violation: ' . \$outcome);
        }
        \$entry = [
            'step_id' => \$row['step_id'] === null ? null : (string) \$row['step_id'],
            'observes' => (\$row['observes'] ?? null) === null ? null : (string) \$row['observes'],
            'kind' => (string) \$row['kind'],
            'outcome' => \$outcome,
        ];
        if (isset(\$row['detail']) && \$row['detail'] !== null) {
            \$entry['detail'] = substr((string) \$row['detail'], 0, 200);
        }
        \$this->assertions[] = \$entry;
    }

    /** Whether any row is unsupported (such a run is never a pass). */
    public function hasUnsupported(): bool
    {
        foreach (\$this->assertions as \$row) {
            if (\$row['outcome'] === 'unsupported') {
                return true;
            }
        }
        return false;
    }

    /** Whether any row failed on an assertion or on infrastructure. */
    public function hasBlockingFailure(): bool
    {
        foreach (\$this->assertions as \$row) {
            if (\$row['outcome'] === 'fail' || \$row['outcome'] === 'infrastructure') {
                return true;
            }
        }
        return false;
    }

    /**
     * Persist the canonical run record into the ingest home; returns
     * its project-relative path.
     */
    public function flush(): string
    {
        \$scenario = \$this->spec['scenario'];
        \$runner = \$this->spec['runner'];
        \$test = \$this->spec['test'];
        \$document = [
            'schema_version' => '$schemaVersion',
            'identity' => '$identity',
            'scenario' => [
                'id' => (string) \$scenario['id'],
                'version' => (string) \$scenario['version'],
                'ir_digest' => (string) \$scenario['irDigest'],
                'symbols' => is_array(\$scenario['symbols'] ?? null) ? \$scenario['symbols'] : [],
                'operations' => is_array(\$scenario['operations'] ?? null) ? \$scenario['operations'] : [],
            ],
            'runner' => [
                'id' => (string) \$runner['id'],
                'version' => (string) \$runner['version'],
            ],
            'profile' => null,
            'test' => [
                'id' => (string) \$test['id'],
                'path' => \$this->relativeTestPath(),
                'fingerprint' => 'sha256:' . hash_file('sha256', \$this->testFile),
            ],
            'binding_mode' => (string) \$this->spec['bindingMode'],
            'started_by' => (string) \$this->spec['startedBy'],
            'assertions' => \$this->assertions,
        ];
        \$root = \$this->projectRoot();
        \$target = \$root . '/' . '$ingestDir' . '/' . \$scenario['id'] . '.json';
        \$dir = dirname(\$target);
        if (!is_dir(\$dir)) {
            mkdir(\$dir, 0777, true);
        }
        file_put_contents(\$target, self::canonicalJson(\$document) . "\\n");
        self::writeToolchainCustody(\$root, \$runner);
        return '$ingestDir' . '/' . \$scenario['id'] . '.json';
    }

    /**
     * Persist the toolchain custody record: the OBSERVED runtime facts
     * of this suite run — the PHP version, the resolved package
     * versions of the conformance stack, and the exact digest of the
     * project's composer.lock. One deterministic document per suite
     * run; an absent package records 'not-installed', an absent lock a
     * null digest — honest absence, never an omission.
     */
    private static function writeToolchainCustody(string \$root, array \$runner): void
    {
        \$packages = [];
        foreach ([$probedPackages] as \$package) {
            \$version = null;
            if (class_exists(\\Composer\\InstalledVersions::class)
                && \\Composer\\InstalledVersions::isInstalled(\$package)) {
                \$version = \\Composer\\InstalledVersions::getPrettyVersion(\$package);
            }
            \$packages[\$package] = is_string(\$version) ? \$version : 'not-installed';
        }
        \$lockPath = \$root . '/composer.lock';
        \$document = [
            'schema_version' => '$toolchainSchema',
            'identity' => '$toolchainIdentity',
            'adapter' => ['id' => '$adapterId', 'version' => '$adapterVersion'],
            'runner' => [
                'id' => (string) \$runner['id'],
                'version' => (string) \$runner['version'],
            ],
            'toolchain' => [
                'php' => PHP_VERSION,
                'composer_lock' => is_file(\$lockPath) ? 'sha256:' . hash_file('sha256', \$lockPath) : null,
                'packages' => \$packages,
            ],
        ];
        \$dir = \$root . '/' . '$toolchainDir';
        if (!is_dir(\$dir)) {
            mkdir(\$dir, 0777, true);
        }
        file_put_contents(\$dir . '/php-laravel.json', self::canonicalJson(\$document) . "\\n");
    }

    /**
     * Canonical JSON over the closed run-record domain: sorted object
     * keys, no whitespace, no escaped slashes, non-ASCII kept literal.
     * The self-contained mirror of the kernel encoder — the emitted
     * reporter never runs inside the adapter process.
     */
    private static function canonicalJson(mixed \$value): string
    {
        if (\$value === null) {
            return 'null';
        }
        if (is_bool(\$value)) {
            return \$value ? 'true' : 'false';
        }
        if (is_int(\$value)) {
            return (string) \$value;
        }
        if (is_string(\$value)) {
            return self::canonicalString(\$value);
        }
        if (!is_array(\$value)) {
            throw new LogicException('run-record value outside the closed canonical domain');
        }
        if (array_is_list(\$value)) {
            return '[' . implode(',', array_map([self::class, 'canonicalJson'], \$value)) . ']';
        }
        \$keys = array_keys(\$value);
        usort(\$keys, 'strcmp');
        \$body = [];
        foreach (\$keys as \$key) {
            \$body[] = self::canonicalString((string) \$key)
                . ':' . self::canonicalJson(\$value[\$key]);
        }
        return '{' . implode(',', \$body) . '}';
    }

    /** The canonical JSON string spelling of one bounded UTF-8 value. */
    private static function canonicalString(string \$value): string
    {
        \$sentinel = str_replace("\\x7F", "\\x00", \$value);
        \$encoded = json_encode(
            \$sentinel,
            JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR,
        );
        return str_replace('\\u0000', "\\x7F", \$encoded);
    }

    /** The project-relative logical path of the importing test file. */
    private function relativeTestPath(): string
    {
        \$root = \$this->projectRoot();
        \$relative = str_replace('\\\\', '/', substr(\$this->testFile, strlen(\$root) + 1));
        return \$relative;
    }

    /**
     * The project root derived from this file's fixed emitted location —
     * never from the current working directory, never from host config.
     */
    private function projectRoot(): string
    {
        \$here = dirname(__FILE__);
        \$root = \$here;
        for (\$index = 0; \$index < self::GENERATED_ROOT_DEPTH; \$index += 1) {
            \$root = dirname(\$root);
        }
        return \$root;
    }
}

PHP;
}

function php_port_text(array $context): string
{
    $header = php_doc_header($context);
    // The FQN travels through the closed string escaper: the validated
    // grammar admits only identifiers and namespace separators, and the
    // escaping keeps even a hostile value from breaking the constant.
    $portClass = php_emit_value($context['portClass']);
    return <<<PHP
$header
// The project test-port binding shim; content depends only on the
// adapter version and the declared port document, so this file is
// itself a determinism probe. Generated file — do not edit.

declare(strict_types=1);

namespace Lekalo\\Generated\\ScenarioTests;

/**
 * Forwarding shim to the project-declared ScenarioPort implementation.
 * The emitted `PORT_CLASS` constant is completed at generation time
 * with the exact project port class name from the validated
 * `lekalo/php-test-port.json` declaration, so the shim itself stays
 * deterministic given the same inputs.
 */
final class Port
{
    private const GENERATED_ROOT_DEPTH = 4;
    private const PORT_CLASS = $portClass;

    /** @var array<string, object> resolved instances, keyed by class */
    private static array \$instances = [];

    /** The resolved project port instance (one per test process). */
    public static function instance(): object
    {
        \$class = self::PORT_CLASS;
        if (isset(self::\$instances[\$class])) {
            return self::\$instances[\$class];
        }
        if (!class_exists(\$class)) {
            throw new LogicException('the declared ScenarioPort class does not exist: ' . \$class);
        }
        return self::\$instances[\$class] = new \$class();
    }

    public static function reset(): void
    {
        self::\$instances = [];
    }
}

PHP;
}

// ---------------------------------------------------------------------------
// Per-scenario test rendering.
// ---------------------------------------------------------------------------

function php_emit_test(array $model, array $context, bool $scaffolded = false): array
{
    $segments = [];
    $cursor = 0;
    $push = static function (string $text, ?string $stepId = null) use (&$segments, &$cursor): void {
        $segments[] = ['text' => $text, 'start' => $cursor, 'stepId' => $stepId];
        $cursor += strlen($text);
    };
    $runnerVersion = $model['runner']['declaredVersion'] ?? null;
    $classFqn = php_test_class_fqn($model['id']);
    $scenarioId = $model['id'];
    $header = php_doc_header($context);
    // The sibling support files travel with every emission (the PHP
    // mirror of the Node emitter's relative import block): the emitted
    // test is self-contained and never depends on project autoload
    // configuration for the generated namespace. A scaffolded test sits
    // in the user-owned scaffold home, four segments below the project
    // root, so its requires walk back to the managed support home.
    $requires = ($scaffolded
        ? "// The support files live in the managed generated home; this file is user-owned.\n"
            . "require_once dirname(__DIR__, 4) . '/src/generated/php-laravel/scenario-tests/scenario-test-kit.php';\n"
            . "require_once dirname(__DIR__, 4) . '/src/generated/php-laravel/scenario-tests/scenario-reporter.php';\n"
            . "require_once dirname(__DIR__, 4) . '/src/generated/php-laravel/scenario-tests/port.php';"
        : "// The sibling support files travel with every generation (the PHP\n"
            . "// mirror of the Node emitter's relative import block): the emitted\n"
            . "// test is self-contained and never depends on project autoload\n"
            . "// configuration for the generated namespace.\n"
            . "require_once __DIR__ . '/../scenario-test-kit.php';\n"
            . "require_once __DIR__ . '/../scenario-reporter.php';\n"
            . "require_once __DIR__ . '/../port.php';");
    // The scaffolded file is user-owned after its one emission: it
    // carries the `lekalo:<id>` claim marker the observed index scans
    // for, plus an explicit edit-freedom note, so the scaffold never
    // masquerades as managed content.
    $marker = $scaffolded
        ? "// lekalo:{$scenarioId} — scaffolded once; edit freely, regeneration never overwrites this file.\n"
        : '';
    // The summary and version interpolate into a // comment of the
    // emitted file: both project through the comment-safe projection so
    // a newline-carrying wire string can never close the comment and
    // inject live code into the generated test.
    $safeVersion = php_comment_safe($model['version'] ?? null);
    $safeSummary = php_comment_safe($model['summary'] ?? null);
    $heredoc = <<<PHP
$header
//
// Scenario {$scenarioId} @{$safeVersion}: {$safeSummary}
// Runner {$model['runner']['id']}; binding {$model['binding']['mode']}; native test id
// php-laravel:{$scenarioId}; generated by the scenario-test compiler (issue #56).
{$marker}
declare(strict_types=1);

namespace Lekalo\\Generated\\ScenarioTests\\{$model['projectId']};

use Lekalo\\Generated\\ScenarioTests\\Port;
use Lekalo\\Generated\\ScenarioTests\\ScenarioReporter;
use Lekalo\\Generated\\ScenarioTests\\ScenarioTestKit;
use Laratesto\\Attribute\\DatabaseMigrations;
use Testo\\Assert;
use Testo\Test;

{$requires}

PHP;
    $push($heredoc, null);
    $blockStart = $cursor;
    // The class carries the `#[Test]` attribute: the canonical
    // attribute-driven discovery of the pinned Testo version. The file
    // spelling stays lowercase (the logical-path grammar forbids
    // uppercase segments) and never relies on the case-suffix
    // convention.
    $push("#[DatabaseMigrations]
#[Test]
" . 'final class ' . php_class_of($scenarioId) . "
{
", null);
    $body = php_render_body($model);
    foreach ($body['segments'] as $segment) {
        // One segment per then-step (the Node emitter's layout): each
        // keeps its step id so the sidecar can declare exact byte
        // ranges per assertion step, never only the whole-class span.
        $push($segment['text'], $segment['stepId']);
    }
    $push("}\n", null);
    $text = implode('', array_map(static fn (array $segment): string => $segment['text'], $segments));
    $leaf = php_scenario_leaf($model['id']);
    $map = [
        'contract' => PHP_MAP_CONTRACT,
        'adapter' => ['id' => PHP_EMITTER_ADAPTER_ID, 'version' => $context['adapterVersion']],
        'owner' => $model['id'],
        'fields' => ['' => $model['id']],
        'declarations' => [
            [
                'id' => $model['id'],
                'kind' => 'scenario',
                'export' => $classFqn,
                'start' => $blockStart,
                'end' => strlen($text),
            ],
            // The scenario leaf scoped under each then-step id is a
            // grammar-valid two-segment Model symbol, unique within the
            // sidecar; the kind and the full step spelling ride beside
            // it, exactly like the Node emitter's map records.
            ...array_map(
                static fn (array $segment): array => [
                    'id' => $leaf . '.' . $segment['stepId'],
                    'kind' => 'then',
                    'step' => $segment['stepId'],
                    'export' => $classFqn,
                    'start' => $segment['start'],
                    'end' => $segment['start'] + strlen($segment['text']),
                ],
                array_values(array_filter(
                    $segments,
                    static fn (array $segment): bool => $segment['stepId'] !== null,
                )),
            ),
        ],
    ];
    return ['text' => $text, 'map' => $map];
}

/** The leaf of one scenario id: the last dot-separated segment. */
function php_scenario_leaf(string $scenarioId): string
{
    $cut = strrpos($scenarioId, '.');
    return $cut === false ? $scenarioId : substr($scenarioId, $cut + 1);
}

/**
 * The per-scenario test body: ONE public `test*` method per scenario
 * (the Laratesto naming convention discovers `*Test.php` classes with
 * `test*` methods). The recorder wraps every recorded row group,
 * unsupported rows short-circuit into a `Skipped` throw (never a
 * pass), and any non-assertion throwable rethrows after flushing
 * (Testo reports it as `Error`, which normalizes to infrastructure
 * evidence).
 *
 * Returns the method as tagged `{text, stepId}` segments: one segment
 * per then-step so the caller's byte ranges can declare per-step
 * ownership, exactly like the Node emitter's segment list.
 */
function php_render_body(array $model): array
{
    $groups = php_render_groups($model);
    $head = '    public function test' . ucfirst(php_identifier_of($model['id'])) . "(): void\n    {\n";
    $head .= "        \$recorder = new ScenarioReporter([\n";
    $head .= "            'scenario' => ['id' => " . php_emit_value($model['id']) . ", 'version' => "
        . php_emit_value($model['version']) . ", 'irDigest' => " . php_emit_value($model['irDigest'] ?? null)
        . ", 'symbols' => [], 'operations' => " . php_emit_value(php_operations_of($model)) . "],\n";
    $head .= "            'runner' => ['id' => " . php_emit_value($model['runner']['id']) . ", 'version' => "
        . php_emit_value($model['runner']['declaredVersion'] ?? PHP_RUNNER_VERSION) . "],\n";
    $head .= "            'test' => ['id' => " . php_emit_value(php_native_test_id($model['id'])) . "],\n";
    $head .= "            'bindingMode' => " . php_emit_value($model['binding']['mode']) . ",\n";
    $head .= "            'startedBy' => 'lekalo-scenario-harness',\n";
    $head .= "        ], __FILE__);\n";
    $head .= "        try {\n";
    // The PHP mirror of the Node emitter's `await resetPort()`: the shim
    // drops its memoized instances, so every test constructs a fresh
    // project port and inherits no in-memory state from a previous test
    // in the same process (the DB lifecycle is the migrations
    // attribute's contract).
    $head .= "            Port::reset();\n";
    $segments = [['text' => $head, 'stepId' => null]];
    foreach ($groups as $group) {
        $text = '';
        foreach ($group['lines'] as $line) {
            $text .= $line . "\n";
        }
        $segments[] = ['text' => $text, 'stepId' => $group['stepId']];
    }
    $tail = "            if (\$recorder->hasUnsupported()) {\n";
    $tail .= "                // Unsupported rows never become passes: skip the test and\n";
    $tail .= "                // let the flushed record carry the exact rows.\n";
    $tail .= "                \$recorder->flush();\n";
    $tail .= "                throw new \\Testo\\Core\\Exception\\SkipTest('scenario.unsupported-capability');\n";
    $tail .= "            }\n";
    $tail .= "            \$recorder->flush();\n";
    $tail .= "        } catch (\\Throwable \$_error) {\n";
    $tail .= "            \$recorder->flush();\n";
    $tail .= "            throw \$_error;\n";
    $tail .= "        }\n";
    $tail .= "    }\n";
    $segments[] = ['text' => $tail, 'stepId' => null];
    return ['segments' => $segments];
}

/** The sorted distinct operation ids of one model's when steps. */
function php_operations_of(array $model): array
{
    $operations = [];
    foreach ($model['when'] as $step) {
        $id = $step['operation']['id'] ?? null;
        if (is_string($id) && !in_array($id, $operations, true)) {
            $operations[] = $id;
        }
    }
    sort($operations, SORT_STRING);
    return $operations;
}

/** The body row groups: given, when, then, in scenario order. */
function php_render_groups(array $model): array
{
    $groups = [];
    $wholeScenarioUnsupported = $model['unsupported'] !== [];
    $stepVars = [];
    $clockIsos = [];
    if ($wholeScenarioUnsupported) {
        // Concurrency race cases and binding-capability gaps: record the
        // declared rows and execute nothing (a serial run would lie).
        $lines = [];
        foreach ($model['unsupported'] as $entry) {
            $lines[] = php_unsupported_row(null, null, 'scenario', $entry['capability'] . ': ' . $entry['reason']);
        }
        foreach ($model['then'] as $step) {
            $lines[] = php_unsupported_row($step['stepId'], $step['observes'], $step['kind'], 'scenario-unsupported');
        }
        $groups[] = ['lines' => $lines, 'stepId' => null];
        return $groups;
    }
    foreach ($model['given'] as $step) {
        if ($step['unsupported'] !== null) {
            $groups[] = [
                'lines' => [php_unsupported_row($step['stepId'], null, 'given:' . $step['kind'],
                    $step['unsupported']['capability'] . ': ' . $step['unsupported']['reason'])],
                'stepId' => null,
            ];
            continue;
        }
        $groups[] = [
            'lines' => array_merge(
                ['    // given ' . $step['stepId'] . ' (' . $step['kind'] . ')'],
                php_render_given($step, $stepVars, $clockIsos),
            ),
            'stepId' => null,
        ];
    }
    foreach ($model['when'] as $step) {
        if ($step['unsupported'] !== null) {
            $groups[] = [
                'lines' => [php_unsupported_row($step['stepId'], null, 'when',
                    $step['unsupported']['capability'] . ': ' . $step['unsupported']['reason'])],
                'stepId' => null,
            ];
            continue;
        }
        $groups[] = [
            'lines' => array_merge(
                ['    // when ' . $step['stepId'] . ' (' . $step['operation']['kind'] . ' ' . $step['operation']['id'] . ')'],
                php_render_when($step, $stepVars),
            ),
            'stepId' => null,
        ];
    }
    foreach ($model['then'] as $step) {
        $groups[] = ['lines' => php_render_then($step, $model, $stepVars, $clockIsos), 'stepId' => $step['stepId']];
    }
    return $groups;
}

function php_unsupported_row(?string $stepId, ?string $observes, string $kind, string $detail): string
{
    return '    $recorder->record(['
        . "'step_id' => " . php_emit_value($stepId) . ', '
        . "'observes' => " . php_emit_value($observes) . ', '
        . "'kind' => " . php_emit_value($kind) . ', '
        . "'outcome' => 'unsupported', "
        . "'detail' => ScenarioTestKit::boundedDetail(" . php_emit_value($detail) . ')]);';
}

function php_render_given(array $step, array &$stepVars, array &$clockIsos): array
{
    $variable = 'given_' . php_identifier_of($step['stepId']);
    $stepVars[$step['stepId']] = $variable;
    $payload = $step['payload'] ?? [];
    switch ($step['kind']) {
        case 'state':
            return [
        '    $' . $variable . ' = Port::instance()->state->seed('
                . php_emit_value($payload['entity'] ?? null) . ', ',
        '        ' . php_emit_value(php_selector_object($payload['selector'] ?? [], $stepVars)) . ', ',
        '        ' . php_emit_value(php_fields_object($payload['fields'] ?? [])) . ');',
            ];
        case 'fixture':
            return [
        '    Port::instance()->fixtures->load('
                . php_emit_value($payload['fixture'] ?? null) . ', ['
                . "'version' => " . php_emit_value($payload['version'] ?? null) . ', '
                . "'capabilities' => " . php_emit_value($payload['capabilities'] ?? []) . ']);',
            ];
        case 'actor':
            return $payload['scope'] === null
                ? ["    \$" . $variable . ' = Port::instance()->actor(' . php_emit_value($payload['actor'] ?? null) . ');']
                : ["    \$" . $variable . ' = Port::instance()->actor('
                    . php_emit_value($payload['actor'] ?? null) . ', '
                    . php_emit_value($payload['scope']) . ');'];
        case 'clock':
            $clockIsos[$step['stepId']] = $payload['at'] ?? null;
            return ['    Port::instance()->clock->freeze(' . php_emit_value($payload['at'] ?? null) . ');'];
        case 'id_source':
            return [
        '    Port::instance()->ids->seed(['
                . "'algorithm' => " . php_emit_value($payload['algorithm'] ?? null) . ', '
                . "'seed' => " . php_emit_value($payload['seed'] ?? null) . ']);',
            ];
        default:
            return ['    // unknown precondition kind ' . $step['kind'] . '; nothing to establish'];
    }
}

/** The emitted selector object of one state precondition. */
function php_selector_object(array $selector, array $stepVars): array
{
    $object = [];
    foreach ($selector as $term) {
        $object[$term['field']] = php_literal_of($term['equals'] ?? null, $stepVars);
    }
    return $object;
}

/** The emitted fields object of one state precondition. */
function php_fields_object(array $fields): array
{
    $object = [];
    $emptyVars = [];
    foreach ($fields as $entry) {
        $object[$entry[0]] = php_literal_of($entry[1] ?? null, $emptyVars);
    }
    return $object;
}

function php_render_when(array $step, array &$stepVars): array
{
    $variable = 'step_' . php_identifier_of($step['stepId']);
    $stepVars[$step['stepId']] = $variable;
    $input = [];
    foreach ($step['input'] as $entry) {
        $leaf = $entry['leaf'];
        $emptyVars = [];
        $input[$entry['field']] = php_literal_of($leaf, $emptyVars);
    }
    $ctx = [];
    if (array_key_exists('actor', $step['ctx'])) {
        $actor = $step['ctx']['actor'];
        $actorId = is_array($actor) ? ($actor['id'] ?? null) : $actor;
        $ctx['actor'] = isset($stepVars[$actorId]) ? ['__stepVar' => $stepVars[$actorId]] : $actorId;
    }
    if (array_key_exists('clock', $step['ctx'])) {
        // The clock ctx references a given clock step's frozen instant;
        // an unestablished reference resolves to null (the mapper has
        // already validated reachability before emission).
        $clockRef = is_array($step['ctx']['clock']) ? ($step['ctx']['clock']['id'] ?? null) : $step['ctx']['clock'];
        $ctx['clock'] = $clockRef;
    }
    if (array_key_exists('idempotencyKey', $step['ctx'])) {
        $emptyVars = [];
        $ctx['idempotencyKey'] = php_literal_of($step['ctx']['idempotencyKey'], $emptyVars);
    }
    return [
        "    \$" . $variable . ' = null;',
        '    try {',
        '        $' . $variable . ' = Port::instance()->invoke('
            . php_emit_value($step['operation']['id']) . ', ',
        '            ' . php_emit_value($input) . ', ',
        '            ' . php_emit_value($ctx) . ');',
        '    } catch (\Throwable $when_error) {',
        '        $recorder->record([' . "'step_id' => " . php_emit_value($step['stepId'])
            . ", 'observes' => null, 'kind' => 'when', 'outcome' => 'infrastructure', "
            . "'detail' => ScenarioTestKit::boundedDetail(\$when_error->getMessage())]);",
        '        throw $when_error;',
        '    }',
    ];
}

function php_render_then(array $step, array $model, array $stepVars, array $clockIsos): array
{
    $observed = $stepVars[$step['observes'] ?? ''] ?? ('step_' . php_identifier_of((string) ($step['observes'] ?? 'run')));
    $meta = "'step_id' => " . php_emit_value($step['stepId'])
        . ", 'observes' => " . php_emit_value($step['observes'])
        . ", 'kind' => " . php_emit_value($step['kind']);
    if ($step['unsupported'] !== null) {
        return [php_unsupported_row($step['stepId'], $step['observes'], $step['kind'],
            $step['unsupported']['capability'] . ': ' . $step['unsupported']['reason'])];
    }
    $checks = php_render_checks($step, $model, $stepVars, $clockIsos, $observed);
    $lines = [
        '    // then ' . $step['stepId'] . ': ' . $step['kind'] . ' over ' . $step['observes'],
        '    try {',
    ];
    foreach ($checks as $check) {
        $lines[] = '        ' . $check;
    }
    $lines[] = '        $recorder->record([' . $meta . ", 'outcome' => 'pass']);";
    $lines[] = '    } catch (\\Testo\\Assert\\State\\Assertion\\AssertionException $then_failure) {';
    $lines[] = '        $recorder->record([' . $meta . ", 'outcome' => 'fail', "
            . "'detail' => ScenarioTestKit::boundedDetail(\$then_failure->getMessage())]);";
    $lines[] = '        throw $then_failure;';
    $lines[] = '    } catch (\Throwable $then_error) {';
    $lines[] = '        $recorder->record([' . $meta . ", 'outcome' => 'infrastructure', "
            . "'detail' => ScenarioTestKit::boundedDetail(\$then_error->getMessage())]);";
    $lines[] = '        throw $then_error;';
    $lines[] = '    }';
    return $lines;
}

/** The check statements of one mapped then step. */
function php_render_checks(array $step, array $model, array $stepVars, array $clockIsos, string $observed): array
{
    $payload = $step['payload'] ?? [];
    switch ($step['kind']) {
        case 'result':
            $checks = ["Assert::same(true, \$" . $observed . "['ok'], ScenarioTestKit::boundedDetail(\$"
                . $observed . "['error']['id'] ?? 'invoke-failed'));"];
            if (array_key_exists('value', $payload)) {
                $emptyVars = [];
                $checks[] = 'Assert::true(ScenarioTestKit::typedEqual($' . $observed
                    . "['value'] ?? null, " . php_emit_value(php_literal_of($payload['value'], $emptyVars)) . '), '
                    . php_emit_value('result-value') . ');';
            }
            return $checks;
        case 'error':
            $checks = [
                'Assert::same(false, $' . $observed . "['ok'], " . php_emit_value('expected a typed error') . ');',
                'Assert::same(' . php_emit_value($payload['error'] ?? null) . ', $' . $observed
                    . "['error']['id'] ?? null, " . php_emit_value('error-id') . ');',
            ];
            foreach ($payload['payload'] ?? [] as $entry) {
                if (($entry['leafProblem'] ?? null) !== null) {
                    continue;
                }
                $emptyVars = [];
                $checks[] = 'Assert::true(ScenarioTestKit::errorFieldsMatch($' . $observed . "['error'] ?? null, ["
                    . php_emit_value($entry['field']) . ' => '
                    . php_emit_value(php_literal_of($entry['leaf'], $emptyVars)) . ']), '
                    . php_emit_value('error-fields') . ');';
            }
            if (!empty($payload['contract'])) {
                $projection = array_map(static fn (array $entry): string => (string) $entry['field'], $payload['payload'] ?? []);
                $checks[] = 'Assert::same(true, Port::instance()->contractCheck('
                    . php_emit_value($payload['contract']) . ', '
                    . php_emit_value($projection) . ', '
                    . '$' . $observed . "['error'] ?? null), " . php_emit_value('error-contract') . ');';
            }
            return $checks;
        case 'entity_state':
            $emptyVars = [];
            $selector = php_emit_value(php_selector_object($payload['where'] ?? [], $emptyVars));
            $exactFields = [];
            $matchFields = [];
            foreach ($payload['fields'] ?? [] as $field => $expectation) {
                if (is_array($expectation) && array_key_exists('match', $expectation)) {
                    $matchFields[] = [$field, $expectation['match']];
                    continue;
                }
                $exactFields[$field] = php_literal_of(
                    is_array($expectation) && array_key_exists('value', $expectation)
                        ? $expectation['value'] : $expectation,
                    $emptyVars,
                );
            }
            $checks = [
                '$stateRows = Port::instance()->state->query('
                    . php_emit_value($payload['entity'] ?? null) . ', ' . $selector . ');',
            ];
            $expect = $payload['expect'] ?? null;
            $count = php_expect_count($expect);
            if ($count === null) {
                $checks[] = "Assert::true(count(\$stateRows) >= 1, " . php_emit_value('entity-exists') . ');';
            } else {
                $checks[] = 'Assert::same(' . var_export($count, true) . ', count($stateRows), '
                    . php_emit_value('entity-count') . ');';
            }
            if ($exactFields !== []) {
                // Per-row projection equals the expected fields object:
                // the PHP mirror of the Node emitter's every-row check.
                $checks[] = 'Assert::true(count(array_filter($stateRows, static fn (array $row): bool => ScenarioTestKit::typedEqual('
                    . "array_intersect_key(\$row, "
                    . php_emit_value(array_fill_keys(array_keys($exactFields), true)) . '), '
                    . php_emit_value($exactFields) . '))) === count($stateRows), '
                    . php_emit_value('entity-fields') . ');';
            }
            foreach ($matchFields as [$field, $matcher]) {
                $check = php_match_check($matcher);
                if ($check === null) {
                    $checks[] = 'Assert::fail(' . php_emit_value('unrenderable match kind ' . $matcher) . ');';
                    continue;
                }
                // The row value is bound into a single-expression closure
                // so the matcher text stays a pure function of one value
                // with no interpolation surface.
                $checks[] = 'foreach ($stateRows as $match_row) { $value = $match_row['
                    . php_emit_value($field) . ']; Assert::true('
                    . str_replace('\\$value', '$value', $check) . ', '
                    . php_emit_value('entity-match:' . $field . ':' . $matcher) . '); }';
            }
            return array_map(
                static fn (string $line): string => ltrim($line),
                $checks,
            );
        case 'emitted':
            $target = $payload['target'] ?? [];
            $countExpr = php_emit_count($payload['count'] ?? null);
            return [
                '$emissions = array_values(array_filter('
                . 'Port::instance()->emissions(), '
                . 'static fn (array $entry): bool => '
                . '($entry[' . "'id'" . '] ?? null) === ' . php_emit_value($target['id'] ?? null)
                . ' && ($entry[' . "'kind'" . '] ?? null) === ' . php_emit_value($target['kind'] ?? null) . '));',
                $countExpr === null
                    ? "Assert::true(count(\$emissions) >= 1, " . php_emit_value('emitted-at-least-one') . ');'
                    : 'Assert::true(count($emissions) ' . $countExpr . ', '
                        . php_emit_value('emitted-count') . ');',
            ];
        case 'forbidden_effect':
            $scopeFilters = [];
            if (($payload['scope'] ?? null) === 'field') {
                $scopeFilters[] = '($entry[' . "'field'" . '] ?? null) === ' . php_emit_value($payload['field'] ?? null);
            }
            return [
                '$matching = array_values(array_filter('
                . 'Port::instance()->effects(), '
                . 'static fn (array $entry): bool => '
                . '($entry[' . "'effect'" . '] ?? null) === ' . php_emit_value($payload['effect'] ?? null)
                . ($scopeFilters !== [] ? ' && ' . implode(' && ', $scopeFilters) : '') . '));',
                'Assert::same(0, count($matching), ' . php_emit_value('forbidden-effect') . ');',
            ];
        case 'authorization':
            return [
                '$decision = Port::instance()->authorize('
                . php_emit_value($payload['actor']['id'] ?? null) . ', '
                . php_emit_value($payload['policy'] ?? null) . ', '
                . php_emit_value(php_observes_operation($model, $step)) . ');',
                'Assert::same(' . php_emit_value($payload['outcome'] ?? null) . ', $decision, '
                    . php_emit_value('authorization-outcome') . ');',
            ];
        case 'idempotency':
            $original = 'step_' . php_identifier_of((string) ($payload['replay'] ?? ''));
            $checks = [
                'Assert::true(ScenarioTestKit::typedEqual($' . $observed . ', $' . $original . '), '
                    . php_emit_value('replay-equivalence') . ');',
            ];
            if (($payload['duplicates'] ?? null) === 'none') {
                $originalStep = null;
                foreach ($model['when'] as $candidate) {
                    if ($candidate['stepId'] === ($payload['replay'] ?? null)) {
                        $originalStep = $candidate;
                        break;
                    }
                }
                if (($originalStep['operation']['id'] ?? null) !== null) {
                    $checks[] = '$original_emissions = array_filter('
                    . 'Port::instance()->emissions(), '
                    . 'static fn (array $entry): bool => '
                    . '($entry[' . "'operation'" . '] ?? null) === '
                    . php_emit_value($originalStep['operation']['id']) . ');';
                    $checks[] = 'Assert::true(count($original_emissions) <= 1, '
                        . php_emit_value('duplicates-none') . ');';
                }
            }
            return array_map(
                static fn (string $line): string => ltrim($line),
                $checks,
            );
        case 'contract_match':
            return [
                '$projection = ' . php_emit_value($payload['projection'] ?? []) . ';',
                '$actual = $' . $observed . "['value'] ?? null;",
                'Assert::same(true, Port::instance()->contractCheck('
                    . php_emit_value($payload['contract'] ?? null) . ', $projection, $actual), '
                    . php_emit_value('contract-match') . ');',
            ];
        case 'deterministic_fixture':
            $fixtureStep = null;
            foreach ($model['given'] as $candidate) {
                if ($candidate['kind'] === 'fixture') {
                    $fixtureStep = $candidate;
                    break;
                }
            }
            if ($fixtureStep === null) {
                return ['Assert::fail(' . php_emit_value('deterministic_fixture without a fixture precondition') . ');'];
            }
            // The digest covers the fixture, the seeded id source, and the
            // frozen clock, so the equality transitively asserts the
            // declared clock/idSource control refs.
            return [
                'Assert::same(' . php_emit_value($payload['digest'] ?? null) . ', '
                . 'Port::instance()->fixtureDigest('
                . php_emit_value($fixtureStep['payload']['fixture'] ?? null) . '), '
                . php_emit_value('fixture-digest') . ');',
            ];
        default:
            return ['Assert::fail(' . php_emit_value('unrenderable assertion kind ' . $step['kind']) . ');'];
    }
}

/** The emitted value check for one closed matcher kind. */
function php_match_check(string $matcher): ?string
{
    return match ($matcher) {
        'uuid' => "preg_match('/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\\$/', (string) \\$value) === 1",
        'datetime' => 'ScenarioTestKit::canonicalDatetime((string) \\$value)',
        'uri' => 'ScenarioTestKit::canonicalUri((string) \\$value)',
        'decimal' => 'ScenarioTestKit::canonicalDecimal((string) \\$value)',
        'non-null' => '\\$value !== null',
        default => null,
    };
}

/** Wrap one matcher expression over `$value` into a full assert line. */
function php_match_assert(string $check, string $label): string
{
    return 'Assert::true((static function (mixed $value): bool { return '
        . $check . '; })(' . '$' . "row['" . str_replace('entity-match:', '', $label) . "'] ?? null" . '), '
        . php_emit_value($label) . ');';
}

/** The observed when step of one then step. */
function php_observes_operation(array $model, array $step): ?string
{
    foreach ($model['when'] as $candidate) {
        if ($candidate['stepId'] === ($step['observes'] ?? null)) {
            return $candidate['operation']['id'] ?? null;
        }
    }
    return null;
}

/** The exact row count expectation, or null for at-least-one. */
function php_expect_count(mixed $expect): int|string|null
{
    if (is_array($expect)) {
        if (array_key_exists('count', $expect)) {
            return $expect['count'];
        }
        if (($expect['presence'] ?? null) === 'missing') {
            return 0;
        }
        if (($expect['presence'] ?? null) === 'exists') {
            return null;
        }
    }
    if (is_int($expect)) {
        return $expect;
    }
    return $expect;
}

/** The emitted comparison operator of one closed count shape. */
function php_emit_count(mixed $count): ?string
{
    if (is_array($count)) {
        if (array_key_exists('exactly', $count)) {
            return '=== ' . var_export($count['exactly'], true);
        }
        if (array_key_exists('atLeast', $count)) {
            return '>= ' . var_export($count['atLeast'], true);
        }
    }
    return null;
}

/**
 * Render one typed leaf (value or reference) into its emitted argument.
 * `step-output` and `given-value` references become the emitted
 * variable bindings of their steps; every other reference kind compiles
 * to its identity string (a port-call argument, never guessed code).
 */
function php_literal_of(mixed $leaf, array &$stepVars = []): mixed
{
    if ($leaf === null || !is_array($leaf) || array_is_list($leaf)) {
        return null;
    }
    if (is_string($leaf['$ref'] ?? null)) {
        if (($leaf['$ref'] === 'step-output' || $leaf['$ref'] === 'given-value')
            && is_string($leaf['id'] ?? null)
            && isset($stepVars[$leaf['id']])) {
            return ['__stepVar' => $stepVars[$leaf['id']]];
        }
        return '$ref:' . $leaf['$ref'] . ':' . ($leaf['id'] ?? 'null');
    }
    switch ($leaf['type'] ?? null) {
        case 'null':
            return null;
        case 'boolean':
        case 'string':
        case 'decimal':
        case 'date':
        case 'datetime':
        case 'uuid':
        case 'uri':
            return $leaf['value'] ?? null;
        case 'integer':
            return is_string($leaf['value'] ?? null) ? (int) $leaf['value'] : $leaf['value'];
        case 'list':
            $items = [];
            foreach ($leaf['value'] ?? [] as $item) {
                $items[] = php_literal_of($item, $stepVars);
            }
            return $items;
        case 'object':
            $object = [];
            foreach ($leaf['value'] ?? [] as $key => $value) {
                $object[$key] = php_literal_of($value, $stepVars);
            }
            return $object;
        default:
            throw new LogicException('unrenderable leaf kind ' . (string) ($leaf['type'] ?? 'null'));
    }
}

/**
 * Render one literal_of output into its exact emitted PHP text: step
 * variables pass through raw, everything else is single-quote encoded
 * with no interpolation (no escaping drift, no code injection).
 */
function php_emit_value(mixed $value): string
{
    if ($value === null) {
        return 'null';
    }
    if (is_bool($value)) {
        return $value ? 'true' : 'false';
    }
    if (is_int($value)) {
        return (string) $value;
    }
    if (is_float($value)) {
        // Closed-wire decimals never ride as floats; a float here is an
        // emitter bug, and emitting a raw float literal would silently
        // weaken the typed contract.
        throw new LogicException('float value in a closed typed position');
    }
    if (is_string($value)) {
        return "'" . str_replace(['\\', "'"], ['\\\\', "\\'"], $value) . "'";
    }
    if (is_array($value) && isset($value['__stepVar'])) {
        return '$' . $value['__stepVar'];
    }
    if (is_array($value) && array_is_list($value)) {
        $items = array_map(__FUNCTION__, $value);
        return '[' . implode(', ', $items) . ']';
    }
    if (is_array($value)) {
        $members = [];
        foreach ($value as $key => $member) {
            $members[] = php_emit_value((string) $key) . ' => ' . php_emit_value($member);
        }
        return '[' . implode(', ', $members) . ']';
    }
    throw new LogicException('unrenderable emitted value');
}

// ----- bundled compiler module: type-policy.php -----

/**
 * The closed type policy and input validation of the PHP type generator
 * (issue #58). Everything in this module is pure validation over plain
 * data: the bounded types-input document (`lekalo/types/*.types.json`,
 * contract `dev.lekalo.php-types-input@0.4.0`) and the compiled project
 * IR evidence it names. Nothing reads the filesystem, nothing writes,
 * nothing executes project code.
 *
 * Defaults are documented in `contracts/php-types-input.schema.v0.4.0.json`
 * and enforced here with the same closed vocabulary: unknown members and
 * unknown enum values refuse. The policy deliberately has no library
 * codec, date library, or Composer member in v0.4.0 — their absence is
 * the explicit unsupported boundary, never a silent fallback.
 */

// ---------------------------------------------------------------------------
// Closed policy vocabulary.
// ---------------------------------------------------------------------------

const PHP_TYPES_INPUT_SCHEMA_VERSION = 'lekalo/php-types-input/v0.4.0';
const PHP_TYPES_INPUT_IDENTITY = 'dev.lekalo.php-types-input@0.4.0';
const PHP_TYPES_MAP_SCHEMA_VERSION = 'lekalo/php-types-map/v0.4.0';
const PHP_TYPES_MAP_IDENTITY = 'dev.lekalo.php-types-map@0.4.0';
const PHP_TYPES_EVIDENCE_SCHEMA_VERSION = 'lekalo/php-types-evidence/v0.4.0';
const PHP_TYPES_EVIDENCE_IDENTITY = 'dev.lekalo.php-types-evidence@0.4.0';
const PHP_TYPES_IR_IDENTITY = 'dev.lekalo.ir@0.2.16';

/** The generated types root (managed custody) and its read/doc input home. */
const PHP_TYPES_GENERATED_ROOT = '.lekalo/generated/php-laravel/types';
const PHP_TYPES_INPUT_HOME = 'lekalo/types';

/**
 * The user-owned scaffold home of type generation (issue #58). The
 * accepted v0.4.0 value is closed: the core lifecycle classifier and the
 * adapter agree on user ownership by this exact path convention, so a
 * policy cannot silently move a scaffold under an unrecognized root.
 */
const PHP_TYPES_SCAFFOLD_ROOT = 'app/lekalo-types';

/** The observed class-shape evidence the checked join consumes. */
const PHP_TYPES_EVIDENCE_PATH = '.lekalo/import/observed/types-evidence.json';

const PHP_TYPES_CUSTODY_MODES = ['managed', 'scaffold-once', 'checked'];
const PHP_TYPES_DEFAULT_NAMESPACE_PREFIX = 'Lekalo\\Generated\\Types';

/** The closed scalar base vocabulary (Model `$defs.scalarDefinition`). */
const PHP_TYPES_SCALAR_BASES = ['string', 'number', 'boolean', 'date', 'datetime', 'uuid', 'uri'];

/** The closed IR definition kinds and which of them map to a type. */
const PHP_TYPES_IR_KINDS = [
    'scalar', 'enum', 'value-object', 'entity', 'command', 'query', 'event',
    'effect', 'endpoint', 'target-binding', 'policy', 'scenario', 'module', 'project',
];
const PHP_TYPES_MAPPED_KINDS = [
    'scalar', 'enum', 'value-object', 'entity', 'command', 'query', 'event',
];

/**
 * The bounded refusal codes of the input join. A refusal is an
 * in-envelope `failed` outcome class, never a guessed plan.
 */
const PHP_TYPES_REFUSALS = [
    'types-input-unreadable',
    'types-input-shape',
    'types-input-identity',
    'types-input-policy',
    'types-input-digest',
    'types-ir-unreadable',
    'types-ir-shape',
    'types-ir-identity',
];

/**
 * Validate one parsed types-input document against its closed shape and
 * return the normalized input `{projectId, irDigest, policy}`, or null
 * when the document is not the accepted contract (a present-but-invalid
 * document is an authoring error, never an all-defaults fallback).
 */
function php_validate_types_input(mixed $document): ?array
{
    if (!is_array($document)) {
        return null;
    }
    if (($document['schemaVersion'] ?? null) !== PHP_TYPES_INPUT_SCHEMA_VERSION
        || ($document['identity'] ?? null) !== PHP_TYPES_INPUT_IDENTITY) {
        return null;
    }
    $projectId = $document['projectId'] ?? null;
    if (!is_string($projectId) || preg_match('/^[a-z][a-z0-9_]*$/', $projectId) !== 1) {
        return null;
    }
    $irDigest = $document['irDigest'] ?? null;
    if (!is_sha256_digest($irDigest)) {
        return null;
    }
    $policy = php_validate_type_policy($document['policy'] ?? null);
    if ($policy === null) {
        return null;
    }
    return ['projectId' => $projectId, 'irDigest' => $irDigest, 'policy' => $policy];
}

/**
 * Validate the closed type policy (issue #58 step 1). `null` policy is
 * the all-defaults policy; a present-but-invalid policy refuses. The
 * custody/scaffold-root pairing is closed: scaffold-once requires the
 * recognized consumer root, and the other modes forbid it outright.
 */
function php_validate_type_policy(mixed $policy): ?array
{
    if ($policy === null) {
        $policy = [];
    }
    if (!is_array($policy)) {
        return null;
    }
    $prefix = PHP_TYPES_DEFAULT_NAMESPACE_PREFIX;
    if (array_key_exists('namespacePrefix', $policy)) {
        $value = $policy['namespacePrefix'];
        if (!is_string($value)
            || preg_match('/^[A-Za-z_][A-Za-z0-9_]*(\\\\[A-Za-z_][A-Za-z0-9_]*){1,7}$/', $value) !== 1) {
            return null;
        }
        $prefix = $value;
    }
    $custody = 'managed';
    if (array_key_exists('custody', $policy)) {
        $value = $policy['custody'];
        if (!is_string($value) || !in_array($value, PHP_TYPES_CUSTODY_MODES, true)) {
            return null;
        }
        $custody = $value;
    }
    $scaffoldRoot = null;
    if (array_key_exists('scaffoldRoot', $policy)) {
        $value = $policy['scaffoldRoot'];
        if ($value !== PHP_TYPES_SCAFFOLD_ROOT) {
            return null;
        }
        $scaffoldRoot = $value;
    }
    if (($custody === 'scaffold-once') !== ($scaffoldRoot !== null)) {
        return null;
    }
    $classMap = true;
    if (array_key_exists('classMap', $policy)) {
        $value = $policy['classMap'];
        if (!is_bool($value)) {
            return null;
        }
        $classMap = $value;
    }
    return [
        'namespacePrefix' => $prefix,
        'custody' => $custody,
        'scaffoldRoot' => $scaffoldRoot,
        'classMap' => $classMap,
    ];
}

/**
 * Structural validation of the consumed compiled-IR evidence. The IR is
 * core-validated upstream, so this check is a bounded trust-but-type
 * gate over exactly the members the mapper consumes, plus the explicit
 * refusals the type policy owes (unknown default metadata, unknown type
 * tags). Returns null when the evidence cannot be typed at all (shape
 * refusal) — the mapper's `default-unsupported` finding covers the
 * well-formed-but-unsupported field metadata case.
 *
 * @return array{definitions: list<array<string, mixed>>}|null
 */
function php_check_ir_document(mixed $ir): ?array
{
    if (!is_array($ir) || ($ir['contract'] ?? null) !== PHP_TYPES_IR_IDENTITY) {
        return null;
    }
    if (!is_array($ir['definitions'] ?? null)) {
        return null;
    }
    foreach ($ir['definitions'] as $definitionIndex => $definition) {
        if (!is_array($definition) || !is_string($definition['id'] ?? null)
            || !is_string($definition['kind'] ?? null)
            || !in_array($definition['kind'], PHP_TYPES_IR_KINDS, true)) {
            return null;
        }
        // The closed symbol grammar is exactly two segments: one module
        // plus one name. Fewer segments cannot address a module namespace;
        // more (or an illegal spelling like a leading digit) would emit
        // an unparseable class.
        if (preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', $definition['id']) !== 1) {
            return null;
        }
        $fields = null;
        switch ($definition['kind']) {
            case 'scalar':
                if (!is_string($definition['base'] ?? null)
                    || !in_array($definition['base'], PHP_TYPES_SCALAR_BASES, true)) {
                    return null;
                }
                break;
            case 'enum':
                if (!is_array($definition['values'] ?? null)) {
                    return null;
                }
                foreach ($definition['values'] as $valueIndex => $value) {
                    if (!is_array($value)) {
                        return null;
                    }
                    // Enum-level default metadata is the same unsupported
                    // case as a field-level default: a bounded finding
                    // with exact provenance, never a shape guess and
                    // never a different refusal class.
                    if (array_key_exists('default', $value)) {
                        throw DefaultMetadataUnsupported::fromField(
                            (string) ($value['value'] ?? '?'),
                            'default',
                        )->withProvenance($definition['id'], '/definitions/' . $definitionIndex . '/values/' . $valueIndex);
                    }
                    if (!is_string($value['value'] ?? null)) {
                        return null;
                    }
                }
                break;
            case 'value-object':
            case 'entity':
                $fields = $definition['fields'] ?? null;
                break;
            case 'command':
                $fields = $definition['input'] ?? null;
                break;
            case 'event':
                $fields = $definition['payload'] ?? null;
                break;
            case 'query':
                if (array_key_exists('returns', $definition)
                    && php_check_type_ref($definition['returns']) === null) {
                    return null;
                }
                break;
        }
        // A structured definition without its field list is out of the
        // closed grammar: bounded types-ir-shape refusal, never a fatal
        // downstream.
        if (in_array($definition['kind'], ['value-object', 'entity', 'command', 'event'], true)
            && !is_array($fields)) {
            return null;
        }
        if (is_array($fields)) {
            $names = [];
            foreach ($fields as $fieldIndex => $field) {
                try {
                    $checked = php_check_ir_field($field);
                } catch (DefaultMetadataUnsupported $unsupported) {
                    // The unsupported default is a bounded mapping
                    // finding with exact provenance, never a fatal.
                    $unsupported->semanticId = $definition['id'];
                    $unsupported->pointer = '/definitions/' . $definitionIndex . '/fields/' . $fieldIndex;
                    throw $unsupported;
                }
                if ($checked === null) {
                    return null;
                }
                $names[] = $checked;
            }
            if (count($names) !== count(array_unique($names))) {
                // Duplicate field names cannot carry a closed wire contract.
                return null;
            }
        }
    }
    return ['definitions' => $ir['definitions']];
}

/**
 * One IR field: closed members only. A `default` member — or any other
 * unconsumed metadata slot — is the explicit unsupported default case:
 * the mapper reports it as a typed finding instead of inventing a
 * defaulting rule.
 *
 * @return string|null the field name, or null when the field is not well-formed
 */
function php_check_ir_field(mixed $field): ?string
{
    if (!is_array($field)) {
        return null;
    }
    $name = $field['name'] ?? null;
    if (!is_string($name) || preg_match('/^[a-z][a-zA-Z0-9_]*$/', $name) !== 1) {
        return null;
    }
    foreach (array_keys($field) as $member) {
        if (!in_array($member, ['name', 'type', 'required', 'description'], true)) {
            // Unknown field metadata (a default, a regex, a range) has no
            // owning contract: refused, never silently ignored.
            throw DefaultMetadataUnsupported::fromField($name, is_string($member) ? $member : '?');
        }
    }
    if (php_check_type_ref($field['type'] ?? null) === null) {
        return null;
    }
    if (array_key_exists('required', $field) && !is_bool($field['required'])) {
        return null;
    }
    if (array_key_exists('description', $field) && !is_string($field['description'])) {
        return null;
    }
    return $name;
}

/**
 * One closed IR type expression: a leaf ref, a list, or an optional,
 * at most one of each wrapper layer (deeper nesting is a mapper
 * unsupported finding, not a shape refusal). Returns the normalized
 * expression or null when the tag is not part of the closed grammar.
 *
 * @return array{leaf: string, list: bool, nullableElements: bool, nullable: bool}|null
 */
function php_check_type_ref(mixed $typeRef): ?array
{
    if (!is_array($typeRef)) {
        return null;
    }
    $keys = array_keys($typeRef);
    if (count($keys) !== 1) {
        return null;
    }
    $tag = $keys[0];
    switch ($tag) {
        case 'ref':
            $leaf = $typeRef['ref'];
            if (!is_string($leaf)
                || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', $leaf) !== 1) {
                return null;
            }
            return ['leaf' => $leaf, 'list' => false, 'nullableElements' => false, 'nullable' => false];
        case 'list':
            $element = php_check_type_ref($typeRef['list'] ?? null);
            if ($element === null || $element['list']) {
                // A nested list wrapper is not a closed wire shape.
                return null;
            }
            // A list of optionals is representable: the element type
            // carries the optionality as nullable elements.
            $element['nullableElements'] = $element['nullable'];
            $element['list'] = true;
            $element['nullable'] = false;
            return $element;
        case 'optional':
            $inner = php_check_type_ref($typeRef['optional'] ?? null);
            if ($inner === null || $inner['nullable']) {
                // Nested optionals collapse no state: refuse closed.
                return null;
            }
            // The whole expression is nullable on top of whatever shape
            // the inner layer carries (leaf, list, or nullable-element list).
            $inner['nullable'] = true;
            return $inner;
        default:
            return null;
    }
}

/**
 * A well-formed field carrying metadata no owning contract defines
 * (issue #58: "Reject unknown default metadata now"). Thrown by the
 * field check; the mapper converts it into the bounded
 * `php-types.mapping-unsupported` finding with reason
 * `default-unsupported`.
 */
final class DefaultMetadataUnsupported extends RuntimeException
{
    /** The owning definition id and exact pointer, attached by the checker. */
    public ?string $semanticId = null;
    public ?string $pointer = null;

    public function __construct(
        public readonly string $fieldName,
        public readonly string $member,
    ) {
        parent::__construct('default-unsupported');
    }

    public static function fromField(string $fieldName, string $member): self
    {
        return new self($fieldName, $member);
    }

    /** Fluent provenance attachment for checkers that know the position. */
    public function withProvenance(string $semanticId, string $pointer): self
    {
        $this->semanticId = $semanticId;
        $this->pointer = $pointer;
        return $this;
    }
}

// ----- bundled compiler module: type-map.php -----

/**
 * The closed type mapping of the PHP generator (issue #58, step 1):
 * one compiled-IR evidence document plus the validated policy map to a
 * closed inventory of PHP constructs — nominal scalar wrappers (opaque
 * ids stay distinct types), string-backed enums in declared order,
 * immutable value objects, entity/command/event DTOs, and query result
 * codecs — with the four presence cases resolved and every unsupported
 * projection reported as a bounded finding BEFORE any byte is planned.
 *
 * The mapping is pure: it reads plain data, it never reads the
 * filesystem, and it never invents a construct the IR does not declare
 * (no maps, no arbitrary unions, no unbound generics, no defaults).
 */

if (!function_exists('php_validate_types_input')) {
    require_once __DIR__ . '/type-policy.php';
}

/** The one wire finding code of unsupported type mapping (issue #58). */
const PHP_TYPES_UNSUPPORTED = 'php-types.mapping-unsupported';

/** The bounded reasons the mapping reports. */
const PHP_TYPES_UNSUPPORTED_REASONS = [
    'recursive-codec-unsupported',
    'nested-optional',
    'default-unsupported',
    'type-unsupported',
    'name-reserved',
    'name-collision',
    'path-collision',
    'module-reserved',
    'ref-unresolved',
];

/**
 * PHP reserved words that must never become a generated class stem
 * (case-insensitive), plus the primitive spellings a nominal wrapper
 * must never shadow. The list is closed; additions are a contract
 * change, not an emission-time guess.
 */
const PHP_TYPES_RESERVED_STEMS = [
    'abstract', 'and', 'array', 'as', 'break', 'callable', 'case', 'catch',
    'class', 'clone', 'const', 'continue', 'declare', 'default', 'do',
    'echo', 'else', 'elseif', 'enum', 'extends', 'false', 'final', 'finally',
    'fn', 'for', 'foreach', 'function', 'global', 'goto', 'if', 'implements',
    'include', 'instanceof', 'interface', 'isset', 'list', 'match', 'namespace',
    'new', 'null', 'or', 'print', 'private', 'protected', 'public', 'readonly',
    'require', 'return', 'static', 'switch', 'throw', 'trait', 'true', 'try',
    'unset', 'use', 'var', 'while', 'xor', 'yield', 'int', 'float', 'string',
    'bool', 'void', 'mixed', 'never', 'object', 'iterable', 'self', 'parent',
];

/**
 * Reserved module namespace segments. `Optional` is the emitted
 * wrapper sub-namespace, so a semantic module spelled `optional`
 * (case-insensitively) would collide with it.
 */
const PHP_TYPES_RESERVED_MODULES = ['optional'];

/** The fixed role suffixes: DTO role suffixes are fixed, never traversal-dependent. */
const PHP_TYPES_KIND_SUFFIXES = [
    'entity' => 'Dto',
    'command' => 'Input',
    'event' => 'Payload',
];

/**
 * Map the compiled IR to the closed type inventory. `input` is
 * `{ir, policy, irDigest, inputDigest}` with `ir` the parsed evidence
 * and `policy` the validated policy. Returns the mapped inventory with
 * one bounded finding per unsupported projection; the caller plans no
 * write while any finding exists.
 */
function php_map_types(array $input): array
{
    $policy = $input['policy'];
    $prefix = $policy['namespacePrefix'];
    try {
        $checked = php_check_ir_document($input['ir']);
    } catch (DefaultMetadataUnsupported $unsupported) {
        return [
            'state' => 'unsupported',
            'findings' => [php_types_finding(
                'default-unsupported',
                $unsupported->semanticId,
                $unsupported->pointer,
                'field `' . $unsupported->fieldName . '` carries unsupported metadata member `' . $unsupported->member . '`',
            )],
            'policy' => $policy,
            'digests' => ['ir' => $input['irDigest'], 'input' => $input['inputDigest']],
        ];
    }
    if ($checked === null) {
        return ['state' => 'refused', 'refusal' => 'types-ir-shape'];
    }
    $definitions = [];
    $order = [];
    foreach ($checked['definitions'] as $index => $definition) {
        $id = $definition['id'];
        if (isset($definitions[$id])) {
            // Duplicate semantic ids cannot carry a closed inventory.
            return ['state' => 'refused', 'refusal' => 'types-ir-shape'];
        }
        $definition['pointer'] = '/definitions/' . $index;
        $definitions[$id] = $definition;
        $order[] = $id;
    }

    $findings = [];
    $addFinding = static function (array $finding) use (&$findings): void {
        $findings[] = $finding;
    };

    // Cycles first: a recursive codec cannot be emitted, so every type
    // on the cycle is unsupported before any naming work happens.
    php_find_type_cycles($definitions, $addFinding);

    $types = [];
    $collections = [];
    $wrappers = [];
    foreach ($order as $id) {
        $definition = $definitions[$id];
        $kind = $definition['kind'];
        if (!in_array($kind, PHP_TYPES_MAPPED_KINDS, true)) {
            // Effects, endpoints, policies, scenarios and the structural
            // kinds are not domain types: no entry, no refusal.
            continue;
        }
        $module = php_types_module_of($id);
        if (in_array(strtolower($module), PHP_TYPES_RESERVED_MODULES, true)) {
            $addFinding(php_types_finding('module-reserved', $id, $definition['pointer'],
                'module namespace segment collides with the emitted wrapper namespace'));
            continue;
        }
        $stem = php_types_stem_of($id, $kind);
        if (in_array(strtolower($stem), PHP_TYPES_RESERVED_STEMS, true)) {
            $addFinding(php_types_finding('name-reserved', $id, $definition['pointer'],
                'class stem is a reserved PHP identifier'));
            continue;
        }
        $entry = [
            'semanticId' => $id,
            'kind' => $kind,
            'fqn' => php_types_fqn_of($prefix, $module, $stem),
            'path' => php_types_path_of($module, $stem),
            'description' => $definition['description'] ?? null,
        ];
        switch ($kind) {
            case 'scalar':
                $entry['base'] = $definition['base'];
                $entry['codec'] = $entry['fqn'];
                $entry['codecPath'] = $entry['path'];
                break;
            case 'enum':
                $cases = php_types_enum_cases($definition, $addFinding);
                if ($cases === null) {
                    continue 2;
                }
                $entry['values'] = $cases;
                $entry['codec'] = $entry['fqn'];
                $entry['codecPath'] = $entry['path'];
                break;
            case 'query':
                $returns = null;
                if (array_key_exists('returns', $definition)) {
                    $returns = php_check_type_ref($definition['returns']);
                }
                if ($returns === null) {
                    $addFinding(php_types_finding('type-unsupported', $id, $definition['pointer'],
                        'query return expression is not a closed mapped shape'));
                    continue 2;
                }
                if (php_types_resolve_expr($returns, $definitions, $id, $definition['pointer'], $addFinding) === null) {
                    continue 2;
                }
                if ($returns['list']) {
                    // A list-valued query return needs its collection class.
                    php_types_register_collection($returns, $definitions, $prefix, $collections);
                }
                // A query declares a direct output, never an envelope: the
                // result artifact is the returns codec passthrough, so the
                // entry's class IS its codec.
                $entry['fqn'] = php_types_fqn_of($prefix, $module, $stem . 'ResultCodec');
                $entry['codec'] = $entry['fqn'];
                $entry['path'] = php_types_path_of($module, $stem . 'ResultCodec');
                $entry['codecPath'] = $entry['path'];
                $entry['returns'] = $returns;
                break;
            default:
                $rawFields = match ($kind) {
                    'command' => $definition['input'],
                    'event' => $definition['payload'],
                    default => $definition['fields'],
                };
                $fields = php_types_map_fields($rawFields, $definitions, $prefix, $id,
                    $definition['pointer'], $collections, $wrappers, $addFinding);
                if ($fields === null) {
                    continue 2;
                }
                $entry['fields'] = $fields;
                $entry['codec'] = $entry['fqn'] . 'Codec';
                $entry['codecPath'] = php_types_path_of($module, php_types_entry_codec_class($entry['codec']));
                break;
        }
        $types[] = $entry;
    }

    $unsupported = static function () use (&$findings, $policy, $input): array {
        return [
            'state' => 'unsupported',
            'findings' => php_types_sort_findings($findings),
            'policy' => $policy,
            'digests' => ['ir' => $input['irDigest'], 'input' => $input['inputDigest']],
        ];
    };

    if ($findings !== []) {
        return $unsupported();
    }

    usort($types, static fn (array $left, array $right): int => strcmp($left['semanticId'], $right['semanticId']));
    usort($collections, static fn (array $left, array $right): int => strcmp($left['fqn'], $right['fqn']));
    usort($wrappers, static fn (array $left, array $right): int => strcmp($left['fqn'], $right['fqn']));

    // Naming custody before any emission: exact duplicate FQNs and
    // case-insensitive path collisions both refuse, because one of the
    // two files would silently shadow the other on a real filesystem.
    $artifacts = php_types_collect_artifacts($types, $collections, $wrappers, $policy, $addFinding);
    if ($findings !== []) {
        return $unsupported();
    }

    $index = [];
    foreach ($types as $entry) {
        $index[$entry['semanticId']] = [
            'fqn' => $entry['fqn'],
            'path' => $entry['path'],
            'codec' => $entry['codec'],
        ];
    }
    return [
        'state' => 'mapped',
        'findings' => [],
        'policy' => $policy,
        'types' => $types,
        'collections' => $collections,
        'wrappers' => $wrappers,
        'artifacts' => $artifacts,
        'index' => $index,
        'digests' => ['ir' => $input['irDigest'], 'input' => $input['inputDigest']],
    ];
}

/** Findings sort deterministically by semantic id, then reason. */
function php_types_sort_findings(array $findings): array
{
    usort($findings, static fn (array $left, array $right): int => strcmp(
        ($left['semanticId'] ?? '|') . '|' . $left['reason'],
        ($right['semanticId'] ?? '|') . '|' . $right['reason'],
    ));
    return $findings;
}

/** One bounded unsupported finding: code, reason, semantic id, pointer. */
function php_types_finding(string $reason, ?string $semanticId, ?string $pointer, string $detail): array
{
    $finding = ['code' => PHP_TYPES_UNSUPPORTED, 'reason' => $reason, 'detail' => $detail];
    if ($semanticId !== null) {
        $finding['semanticId'] = $semanticId;
    }
    if ($pointer !== null) {
        $finding['pointer'] = $pointer;
    }
    return $finding;
}

/** The module segment of one semantic id (the prefix before the first dot). */
function php_types_module_of(string $semanticId): string
{
    $cut = strpos($semanticId, '.');
    return $cut === false || $cut === 0 ? $semanticId : substr($semanticId, 0, $cut);
}

/** The last dot segment of one semantic id. */
function php_types_leaf_name_of(string $semanticId): string
{
    $cut = strrpos($semanticId, '.');
    return $cut === false ? $semanticId : substr($semanticId, $cut + 1);
}

/**
 * The Pascal-case class stem of one symbol: the final id segment splits
 * on `_`, every part capitalizes, and structured kinds append their
 * fixed role suffix. `planner.task_id` → `TaskId`;
 * `planner.task` (entity) → `TaskDto`; `planner.focus_task` (command)
 * → `FocusTaskInput`; `planner.task_focused` (event) →
 * `TaskFocusedPayload`; `planner.count_focused` (query) →
 * `CountFocusedResult`.
 */
function php_types_stem_of(string $semanticId, string $kind): string
{
    $sanitized = preg_replace('/[^a-zA-Z0-9_]/', '_', php_types_leaf_name_of($semanticId));
    $parts = explode('_', (string) $sanitized);
    $stem = implode('', array_map(
        static fn (string $part): string => ucfirst($part),
        $parts,
    ));
    return $stem . (PHP_TYPES_KIND_SUFFIXES[$kind] ?? '');
}

/** The deterministic camel-case property spelling of one wire name. */
function php_types_property_of(string $wireName): string
{
    $sanitized = preg_replace('/[^a-zA-Z0-9_]/', '_', $wireName);
    $parts = explode('_', (string) $sanitized);
    $first = array_shift($parts);
    return $first . implode('', array_map(
        static fn (string $part): string => ucfirst($part),
        $parts,
    ));
}

/** The full FQN of one generated class. */
function php_types_fqn_of(string $prefix, string $module, string $class): string
{
    return $prefix . '\\' . ucfirst($module) . '\\' . $class;
}

/**
 * The snake-case file spelling of one class (or namespace-qualified
 * class): the wire path grammar is lowercase, so the emitted paths
 * mirror the FQN deterministically (`TaskId` → `task_id.php`,
 * `Optional\OptionalDueDate` → `optional/optional_due_date.php`).
 */
function php_types_snake_of(string $spelling): string
{
    $withSlashes = str_replace('\\', '/', $spelling);
    $snake = preg_replace('/([a-z0-9])([A-Z])/', '$1_$2', $withSlashes);
    return strtolower((string) $snake);
}

/**
 * The artifact path of one generated class, relative to the generated
 * root: the lowercase module directory plus the snake-cased class
 * spelling. PHP-identifier case stays in the FQN, never in the path.
 */
function php_types_path_of(string $module, string $class): string
{
    return strtolower($module) . '/' . php_types_snake_of($class) . '.php';
}

/**
 * The enum cases of one enum definition: declared values preserved
 * verbatim, case names derived deterministically, declared order kept.
 * A case-name derivation collision (two values normalizing to one
 * identifier) is a bounded naming finding, never a silent merge.
 */
function php_types_enum_cases(array $definition, callable $addFinding): ?array
{
    $cases = [];
    $byName = [];
    foreach ($definition['values'] as $value) {
        $raw = (string) $value['value'];
        $name = ucfirst((string) preg_replace('/[^a-zA-Z0-9]/', '_', $raw));
        if ($name === '' || preg_match('/^[A-Za-z_][A-Za-z0-9_]*$/', $name) !== 1
            || in_array(strtolower($name), PHP_TYPES_RESERVED_STEMS, true)) {
            $addFinding(php_types_finding('name-reserved', $definition['id'], $definition['pointer'],
                'enum value does not normalize to a usable PHP identifier'));
            return null;
        }
        if (isset($byName[strtolower($name)])) {
            $addFinding(php_types_finding('name-collision', $definition['id'], $definition['pointer'],
                'enum case name collision on ' . $name));
            return null;
        }
        $byName[strtolower($name)] = true;
        $cases[] = ['case' => $name, 'value' => $raw];
    }
    return $cases;
}

/**
 * Map one definition's fields to closed field rows: verbatim wire
 * name, camel-case property, normalized type expression, and exactly
 * one of the four presence cases. List positions register collection
 * classes; optional positions register presence wrappers; unknown or
 * unresolved references are bounded findings. Returns null when the
 * definition is unsupported as a whole.
 */
function php_types_map_fields(
    array $rawFields,
    array $definitions,
    string $prefix,
    string $ownerId,
    string $pointer,
    array &$collections,
    array &$wrappers,
    callable $addFinding,
): ?array {
    $fields = [];
    $properties = [];
    foreach ($rawFields as $fieldIndex => $field) {
        try {
            $expr = php_check_type_ref($field['type'] ?? null);
        } catch (DefaultMetadataUnsupported $unsupported) {
            $addFinding(php_types_finding(
                'default-unsupported',
                $ownerId,
                $pointer . '/fields/' . $fieldIndex,
                'field `' . $field['name'] . '` carries unsupported metadata member `' . $unsupported->member . '`',
            ));
            return null;
        }
        if ($expr === null) {
            $addFinding(php_types_finding('type-unsupported', $ownerId,
                $pointer . '/fields/' . $fieldIndex,
                'field `' . $field['name'] . '` is not a closed mapped shape'));
            return null;
        }
        if (php_types_resolve_expr($expr, $definitions, $ownerId,
            $pointer . '/fields/' . $fieldIndex, $addFinding) === null) {
            return null;
        }
        $required = $field['required'] ?? false;
        $presence = match (true) {
            $required && !$expr['nullable'] => 'required-nonnull',
            $required && $expr['nullable'] => 'required-nullable',
            !$required && !$expr['nullable'] => 'optional-nonnull',
            default => 'optional-nullable',
        };
        $property = php_types_property_of($field['name']);
        if (isset($properties[strtolower($property)])) {
            $addFinding(php_types_finding('name-collision', $ownerId,
                $pointer . '/fields/' . $fieldIndex,
                'property spelling collision on ' . $property));
            return null;
        }
        $properties[strtolower($property)] = true;
        $collectionClass = null;
        if ($expr['list']) {
            $collectionClass = php_types_register_collection($expr, $definitions, $prefix, $collections);
        }
        $wrapper = null;
        if ($presence === 'optional-nonnull' || $presence === 'optional-nullable') {
            // Presence wrappers exist only for optional positions: the
            // required-nullable case is a plain `?T` constructor type.
            // The registration return is the authority — on a dedup hit
            // the existing wrapper binds, never the last-appended row.
            $wrapper = php_types_register_wrapper($expr, $definitions, $prefix, $collectionClass,
                $presence === 'optional-nullable', $wrappers);
        }
        $row = [
            'name' => $field['name'],
            'property' => $property,
            'type' => [
                'leaf' => $expr['leaf'],
                'list' => $expr['list'],
                'nullable' => $expr['nullable'],
            ],
            'presence' => $presence,
            'nullable' => $expr['nullable'],
        ];
        if ($expr['list']) {
            $row['type']['collection'] = $collectionClass['fqn'];
            if ($expr['nullableElements']) {
                $row['type']['nullableElements'] = true;
            }
        }
        if ($wrapper !== null) {
            $row['type']['wrapper'] = $wrapper['fqn'];
        }
        $fields[] = $row;
    }
    return $fields;
}

/**
 * Resolve one normalized expression against the definition index: the
 * leaf must exist and map to a type. Returns null (with a finding)
 * when the reference is unresolved.
 */
function php_types_resolve_expr(
    array $expr,
    array $definitions,
    string $ownerId,
    string $pointer,
    callable $addFinding,
): ?array {
    $leaf = $expr['leaf'];
    $definition = $definitions[$leaf] ?? null;
    if ($definition === null || !in_array($definition['kind'], PHP_TYPES_MAPPED_KINDS, true)) {
        $addFinding(php_types_finding('ref-unresolved', $ownerId, $pointer,
            'reference `' . $leaf . '` does not resolve to a mapped type'));
        return null;
    }
    return $expr;
}

/** The bare codec class name of one mapped entry (the final FQN segment). */
function php_types_entry_codec_class(string $codecFqn): string
{
    $cut = strrpos($codecFqn, '\\');
    return $cut === false ? $codecFqn : substr($codecFqn, $cut + 1);
}

/**
 * Register (or find) the immutable collection class one list position
 * needs. Keyed by element and element nullability, so the same list
 * shape shares one class across every definition.
 */
function php_types_register_collection(
    array $expr,
    array $definitions,
    string $prefix,
    array &$collections,
): array {
    $leaf = $expr['leaf'];
    $nullableElements = $expr['nullableElements'];
    $key = $leaf . '|' . ($nullableElements ? 'nullable' : 'plain');
    foreach ($collections as $collection) {
        if ($collection['key'] === $key) {
            return $collection;
        }
    }
    $definition = $definitions[$leaf];
    $module = php_types_module_of($leaf);
    $class = php_types_stem_of($leaf, $definition['kind']) . 'List';
    if ($nullableElements) {
        // The null-permitting twin of a list class keeps its own name so
        // both variants can coexist collision-free.
        $class = php_types_stem_of($leaf, $definition['kind']) . 'NullableList';
    }
    $collection = [
        'key' => $key,
        'element' => $leaf,
        'elementKind' => $definition['kind'],
        'nullableElements' => $nullableElements,
        'class' => $class,
        'fqn' => php_types_fqn_of($prefix, $module, $class),
        'path' => php_types_path_of($module, $class),
    ];    $collections[] = $collection;
    return $collection;
}

/**
 * Register (or find) the concrete presence wrapper one optional
 * position needs. `acceptsNull` separates the optional-nonnull flavor
 * (absent | value, null refused) from the optional-nullable flavor
 * (absent | null | value, three distinct states).
 */
function php_types_register_wrapper(
    array $expr,
    array $definitions,
    string $prefix,
    ?array $collection,
    bool $acceptsNull,
    array &$wrappers,
): array {
    $leaf = $expr['leaf'];
    $key = $leaf . '|' . ($collection !== null ? 'list' : 'plain') . '|' . ($acceptsNull ? 'nullable' : 'plain');
    foreach ($wrappers as $wrapper) {
        if ($wrapper['key'] === $key) {
            return $wrapper;
        }
    }
    $definition = $definitions[$leaf];
    $module = php_types_module_of($leaf);
    $stem = $collection !== null ? $collection['class'] : php_types_stem_of($leaf, $definition['kind']);
    $class = 'Optional' . ($acceptsNull ? 'Nullable' : '') . $stem;
    // Wrapper classes live in the module's Optional sub-namespace.
    $wrapper = [
        'key' => $key,
        'element' => $leaf,
        'elementKind' => $definition['kind'],
        'ofList' => $collection !== null,
        'acceptsNull' => $acceptsNull,
        'class' => $class,
        'fqn' => php_types_fqn_of($prefix, $module, 'Optional\\' . $class),
        'path' => php_types_path_of($module, 'Optional\\' . $class),
        'collection' => $collection['fqn'] ?? null,
    ];
    $wrappers[] = $wrapper;
    return $wrapper;
}

/**
 * Detect reference cycles among the structured definitions (a codec
 * for a recursive type cannot terminate). Every type on a cycle gets
 * one bounded finding; the emitter stays silent.
 */
function php_find_type_cycles(array $definitions, callable $addFinding): void
{
    $state = []; // 1 = open, 2 = done
    $stack = [];
    $visit = static function (string $id) use (&$visit, &$state, &$stack, $definitions, $addFinding): void {
        if (($state[$id] ?? 0) === 2) {
            return;
        }
        if (($state[$id] ?? 0) === 1) {
            $cycle = array_slice($stack, (int) array_search($id, $stack, true));
            $cycle[] = $id;
            foreach ($cycle as $member) {
                $definition = $definitions[$member];
                $addFinding(php_types_finding('recursive-codec-unsupported', $member,
                    $definition['pointer'],
                    'recursive reference cycle: ' . implode(' -> ', $cycle)));
            }
            return;
        }
        $state[$id] = 1;
        $stack[] = $id;
        $definition = $definitions[$id];
        $fields = match ($definition['kind']) {
            'command' => $definition['input'] ?? [],
            'event' => $definition['payload'] ?? [],
            'value-object', 'entity' => $definition['fields'] ?? [],
            default => [],
        };
        foreach ($fields as $field) {
            try {
                $expr = php_check_type_ref($field['type'] ?? null);
            } catch (DefaultMetadataUnsupported) {
                // The field mapper owns that refusal; the cycle walk
                // only needs the reference graph.
                continue;
            }
            if ($expr === null) {
                continue;
            }
            $target = $definitions[$expr['leaf']] ?? null;
            if ($target !== null && in_array($target['kind'], ['value-object', 'entity', 'command', 'event'], true)) {
                $visit($expr['leaf']);
            }
        }
        array_pop($stack);
        $state[$id] = 2;
    };
    foreach (array_keys($definitions) as $id) {
        if (in_array($definitions[$id]['kind'], ['value-object', 'entity', 'command', 'event'], true)) {
            $visit($id);
        }
    }
}

/**
 * The complete path-sorted artifact inventory of one mapped inventory,
 * with naming custody applied (duplicate FQN and case-insensitive
 * path collisions are bounded findings).
 */
function php_types_collect_artifacts(
    array $types,
    array $collections,
    array $wrappers,
    array $policy,
    callable $addFinding,
): array {
    $artifacts = [];
    $byFqn = [];
    $byPath = [];
    $record = static function (array $artifact) use (&$artifacts, &$byFqn, &$byPath, $addFinding): void {
        $lowerPath = strtolower($artifact['path']);
        if (isset($artifact['fqn'])) {
            $lowerFqn = strtolower($artifact['fqn']);
            if (isset($byFqn[$lowerFqn])) {
                $addFinding(php_types_finding('name-collision', $artifact['semanticId'] ?? null, null,
                    'duplicate class FQN ' . $artifact['fqn']));
                return;
            }
            $byFqn[$lowerFqn] = true;
        }
        if (isset($byPath[$lowerPath])) {
            $addFinding(php_types_finding('path-collision', $artifact['semanticId'] ?? null, null,
                'case-insensitive artifact path collision ' . $artifact['path']));
            return;
        }
        $byPath[$lowerPath] = true;
        $artifacts[] = $artifact;
    };
    foreach ($types as $entry) {
        if ($entry['kind'] === 'query') {
            // The query result codec is the artifact itself (issue #50:
            // the routes family encodes list-return queries through it),
            // so the classmap must carry it like any other class.
            $record([
                'path' => $entry['path'],
                'fqn' => $entry['fqn'],
                'role' => 'codec',
                'semanticId' => $entry['semanticId'],
            ]);
            continue;
        }
        $record([
            'path' => $entry['path'],
            'fqn' => $entry['fqn'],
            'role' => 'type',
            'semanticId' => $entry['semanticId'],
        ]);
        if ($entry['codecPath'] !== $entry['path']) {
            // Scalar wrappers and enums are their own codec: one file,
            // one artifact row.
            $record([
                'path' => $entry['codecPath'],
                'fqn' => $entry['codec'],
                'role' => 'codec',
                'semanticId' => $entry['semanticId'],
            ]);
        }
    }
    foreach ($collections as $collection) {
        $record([
            'path' => $collection['path'],
            'fqn' => $collection['fqn'],
            'role' => 'collection',
            'semanticId' => $collection['element'],
            'element' => $collection['element'],
        ]);
    }
    foreach ($wrappers as $wrapper) {
        $record([
            'path' => $wrapper['path'],
            'fqn' => $wrapper['fqn'],
            'role' => 'optional',
            'semanticId' => $wrapper['element'],
            'element' => $wrapper['element'],
        ]);
    }
    if ($policy['classMap']) {
        $record([
            'path' => 'classmap.php',
            'fqn' => $policy['namespacePrefix'] . '\\ClassMap',
            'role' => 'class-map',
        ]);
    }
    $record([
        'path' => 'types.map.json',
        'role' => 'document',
    ]);
    usort($artifacts, static fn (array $left, array $right): int => strcmp($left['path'], $right['path']));
    return $artifacts;
}

// ----- bundled compiler module: type-codec.php -----

/**
 * The codec emitter of the PHP type generator (issue #58, step 2): one
 * deterministic codec per mapped type. Decoding validates the closed
 * wire shape — unknown members, missing required members, wrong
 * primitives, precision loss and unknown enum values refuse — and
 * encoding reproduces canonical JSON semantics: absent stays absent,
 * explicit null stays null, lists stay lists, objects stay objects,
 * and wire names stay verbatim (`task_id` never becomes `taskId` on
 * the wire).
 */

if (!function_exists('php_map_types')) {
    require_once __DIR__ . '/type-map.php';
}

/**
 * The class FQN of one leaf reference (the nominal wrapper, the enum,
 * or the DTO/value-object class).
 */
function php_types_leaf_fqn(array $definitions, string $prefix, string $leaf): string
{
    $definition = $definitions[$leaf];
    return php_types_fqn_of($prefix, php_types_module_of($leaf), php_types_stem_of($leaf, $definition['kind']));
}

/** Whether one leaf decodes through its own class (scalar, enum). */
function php_types_leaf_is_self_codec(array $definitions, string $leaf): bool
{
    return in_array($definitions[$leaf]['kind'], ['scalar', 'enum'], true);
}

/**
 * The decode expression of one wire value at a resolved leaf: scalars
 * and enums decode through their own class, structured values through
 * their codec class.
 */
function php_types_leaf_decode(array $definitions, string $prefix, string $leaf, string $raw): string
{
    $fqn = '\\' . php_types_leaf_fqn($definitions, $prefix, $leaf);
    return php_types_leaf_is_self_codec($definitions, $leaf)
        ? $fqn . '::fromWire(' . $raw . ')'
        : $fqn . 'Codec::decode(' . $raw . ')';
}

/**
 * The encode expression of one typed leaf value: the exact wire
 * projection (enum backed value, wrapper spelling, codec delegation).
 */
function php_types_leaf_encode(array $definitions, string $prefix, string $leaf, string $target): string
{
    $definition = $definitions[$leaf];
    if ($definition['kind'] === 'enum') {
        return $target . '->toWire()';
    }
    if ($definition['kind'] === 'scalar') {
        return $target . (($definition['base'] === 'number' || $definition['base'] === 'boolean') ? '->value()' : '->toString()');
    }
    return '\\' . php_types_leaf_fqn($definitions, $prefix, $leaf) . 'Codec::encode(' . $target . ')';
}

/**
 * The PHP property type of one mapped field row: the concrete class,
 * collection, or presence wrapper the position binds to. A
 * required-nullable position spells `?T`; an optional position spells
 * its wrapper (the wrapper owns the optionality).
 */
function php_types_field_php_type(array $field, array $definitions, string $prefix): string
{
    $expr = $field['type'];
    if (isset($expr['wrapper'])) {
        return '\\' . $expr['wrapper'];
    }
    if ($expr['list']) {
        $type = '\\' . $expr['collection'];
    } else {
        $type = '\\' . php_types_leaf_fqn($definitions, $prefix, $expr['leaf']);
    }
    return $field['nullable'] ? '?' . $type : $type;
}

/** The single-quoted PHP string literal of one wire text. */
function php_types_string_literal(string $value): string
{
    return "'" . str_replace(["\\", "'"], ["\\\\", "\\'"], $value) . "'";
}

/**
 * The decode fragment of one field: statements binding `$<property>`
 * to the decoded PHP value. Presence is exact — required-nonnull
 * refuses absent and null; required-nullable refuses absent, accepts
 * null; optional positions bind a concrete presence wrapper.
 */
function php_types_field_decode(array $field, array $definitions, string $prefix, string $ownerId): string
{
    $wireKey = php_types_string_literal($field['name']);
    $owner = php_types_string_literal($ownerId);
    $member = $field['name'];
    $wire = '$wire[' . $wireKey . ']';
    $property = '$' . $field['property'];
    $expr = $field['type'];
    $lines = [];

    $valueDecode = static function (string $raw) use ($definitions, $prefix, $expr, $field): string {
        if ($expr['list']) {
            return 'self::decode' . ucfirst($field['property']) . '(' . $raw . ')';
        }
        return php_types_leaf_decode($definitions, $prefix, $expr['leaf'], $raw);
    };

    switch ($field['presence']) {
        case 'required-nonnull':
            $lines[] = 'if (!array_key_exists(' . $wireKey . ', $wire) || ' . $wire . ' === null) {';
            $lines[] = '    throw new \\InvalidArgumentException(' . $owner . ' . \': missing required member `' . $member . '`\');';
            $lines[] = '}';
            $lines[] = $property . ' = ' . $valueDecode($wire) . ';';
            break;
        case 'required-nullable':
            $lines[] = 'if (!array_key_exists(' . $wireKey . ', $wire)) {';
            $lines[] = '    throw new \\InvalidArgumentException(' . $owner . ' . \': missing required member `' . $member . '`\');';
            $lines[] = '}';
            $lines[] = $property . ' = ' . $wire . ' === null ? null : ' . $valueDecode($wire) . ';';
            break;
        case 'optional-nonnull':
        case 'optional-nullable':
            $wrapper = '\\' . $expr['wrapper'];
            $lines[] = $property . ' = !array_key_exists(' . $wireKey . ', $wire)';
            $lines[] = '    ? ' . $wrapper . '::absent()';
            if ($field['presence'] === 'optional-nullable') {
                $lines[] = '    : (' . $wire . ' === null';
                $lines[] = '        ? ' . $wrapper . '::ofNull()';
                $lines[] = '        : ' . $wrapper . '::of(' . $valueDecode($wire) . '));';
            } else {
                // An explicit null fails inside the element decode.
                $lines[] = '    : ' . $wrapper . '::of(' . $valueDecode($wire) . ');';
            }
            break;
    }
    return implode("\n", $lines);
}

/**
 * The encode fragment of one field: statements appending the verbatim
 * wire member to `$result` — absent optionals append nothing, explicit
 * nulls append null, and the member order is the declared field order.
 */
function php_types_field_encode(array $field, array $definitions, string $prefix): string
{
    $wireKey = php_types_string_literal($field['name']);
    $property = '$value->' . $field['property'];
    $expr = $field['type'];
    $lines = [];

    $valueEncode = static function (string $target) use ($definitions, $prefix, $expr, $field): string {
        if ($expr['list']) {
            return 'self::encode' . ucfirst($field['property']) . '(' . $target . ')';
        }
        return php_types_leaf_encode($definitions, $prefix, $expr['leaf'], $target);
    };

    switch ($field['presence']) {
        case 'required-nonnull':
            $lines[] = '$result[' . $wireKey . '] = ' . $valueEncode($property) . ';';
            break;
        case 'required-nullable':
            $lines[] = '$result[' . $wireKey . '] = ' . $property . ' === null ? null : ' . $valueEncode($property) . ';';
            break;
        case 'optional-nonnull':
        case 'optional-nullable':
            $lines[] = 'if (!$value->' . $field['property'] . '->isAbsent()) {';
            if ($field['presence'] === 'optional-nullable') {
                $lines[] = '    $result[' . $wireKey . '] = $value->' . $field['property'] . '->isNull() ? null : '
                    . $valueEncode($property . '->get()') . ';';
            } else {
                $lines[] = '    $result[' . $wireKey . '] = ' . $valueEncode($property . '->get()') . ';';
            }
            $lines[] = '}';
            break;
    }
    return implode("\n", $lines);
}

/**
 * The private list decode/encode helper pair of one list field (or of
 * a query return). Element decoding validates every element; the
 * collection constructor is the only list publisher.
 *
 * Accepted wire boundary: the codec consumes PHP-decoded JSON values,
 * where a JSON object and an empty list are both `[]` and `"0":…`
 * loses its object-ness. A wire object therefore decodes as an empty
 * (or numerically keyed) list at this layer; duplicate members and
 * envelope-level shape custody belong to the kernel's JSON boundary
 * (`decode_json_document`), which refuses duplicates before any codec
 * runs (issue #58).
 */
function php_types_list_helpers(array $field, array $definitions, string $prefix, string $ownerId): string
{
    $expr = $field['type'];
    $collection = '\\' . $expr['collection'];
    $method = ucfirst($field['property']);
    $owner = php_types_string_literal($ownerId);
    $member = $field['name'];
    $nullableElements = $expr['nullableElements'] ?? false;

    $elementDecode = $nullableElements
        ? '$element === null ? null : ' . php_types_leaf_decode($definitions, $prefix, $expr['leaf'], '$element')
        : php_types_leaf_decode($definitions, $prefix, $expr['leaf'], '$element');
    $elementEncode = $nullableElements
        ? '$element === null ? null : ' . php_types_leaf_encode($definitions, $prefix, $expr['leaf'], '$element')
        : php_types_leaf_encode($definitions, $prefix, $expr['leaf'], '$element');

    return <<<PHP
    private static function decode{$method}(mixed \$raw): {$collection}
    {
        if (!is_array(\$raw) || !array_is_list(\$raw)) {
            throw new \\InvalidArgumentException({$owner} . ': member `{$member}` is not a list');
        }
        \$items = [];
        foreach (\$raw as \$element) {
            \$items[] = {$elementDecode};
        }
        return {$collection}::fromList(\$items);
    }

    private static function encode{$method}({$collection} \$value): array
    {
        \$items = [];
        foreach (\$value->all() as \$element) {
            \$items[] = {$elementEncode};
        }
        return \$items;
    }
PHP;
}

// ----- bundled compiler module: type-emit.php -----

/**
 * The emitter of the PHP type generator (issue #58, step 2): the mapped
 * inventory becomes deterministic PHP 8.3 files — final readonly
 * classes, native string-backed enums, typed immutable collections,
 * concrete presence wrappers, and one codec per type — plus the
 * deterministic class map and the authoritative mapping sidecar.
 *
 * Every file declares `strict_types=1`, carries its semantic id, and
 * escapes free wire text through a comment-safe projection. There are
 * no timestamps, absolute paths, environment lookups, or dynamic
 * members anywhere in the output: repeated generations over the same
 * inputs are byte-identical.
 */

if (!function_exists('php_types_leaf_fqn')) {
    require_once __DIR__ . '/type-codec.php';
}

/**
 * One comment-safe single-line projection of free wire text: every
 * line terminator and control character collapses, so a core-valid
 * description can never close a generated comment and inject live code
 * into the emitted class.
 */
function php_types_comment_safe(mixed $text): string
{
    $value = (string) ($text ?? '');
    $value = preg_replace('/\r\n|[\r\n\x{0085}\x{2028}\x{2029}]|\p{Cc}/u', ' ', $value) ?? '';
    $value = preg_replace('/\s+/', ' ', $value) ?? '';
    $value = trim($value);
    // Byte-exact truncation keeps the emitted bytes a pure function of
    // the input bytes (and keeps php -n runtimes safe).
    $value = substr($value, 0, 200);
    // A one-line comment also ends at the mid-line close-tag pair: break
    // it so no projection can re-open PHP mode.
    return str_replace('?>', '? >', $value);
}

/**
 * The canonical JSON spelling of the sidecar documents: sorted object
 * keys, compact separators.
 */
function php_types_canonical_json(mixed $value): string
{
    if (is_array($value)) {
        // An empty array is always an empty LIST in this document family:
        // the sidecar's array slots (types, artifacts, fields, values)
        // are list-typed by the contract, so `[]` must never spell the
        // empty object `{}` (which would violate the closed schema).
        if (array_is_list($value)) {
            return '[' . implode(',', array_map(
                static fn ($item): string => php_types_canonical_json($item),
                $value,
            )) . ']';
        }
        $keys = array_keys($value);
        sort($keys, SORT_STRING);
        $members = [];
        foreach ($keys as $key) {
            $members[] = json_encode((string) $key, JSON_UNESCAPED_SLASHES)
                . ':' . php_types_canonical_json($value[$key]);
        }
        return '{' . implode(',', $members) . '}';
    }
    if (is_bool($value)) {
        return $value ? 'true' : 'false';
    }
    if ($value === null) {
        return 'null';
    }
    if (is_int($value) || is_float($value)) {
        return json_encode($value);
    }
    return json_encode((string) $value, JSON_UNESCAPED_SLASHES);
}

/**
 * The provenance header of every emitted file: no timestamp, no
 * absolute path — digests only, so the bytes stay a pure function of
 * the inputs.
 */
function php_types_file_header(array $context, string $namespace, string $semanticId, ?string $description): string
{
    $lines = [
        '<?php',
        '',
        'declare(strict_types=1);',
        '',
        '// Generated by lekalo-target-php-laravel@' . $context['adapterVersion']
            . ' (type generator, issue #58).',
        '// From ' . PHP_TYPES_IR_IDENTITY . ' input ' . $context['irDigest'] . '.',
        '// Semantic id: ' . $semanticId . '.',
    ];
    if ($description !== null && $description !== '') {
        $lines[] = '// Description: ' . php_types_comment_safe($description);
    }
    // Custody wording follows the declared mode: managed bytes are
    // regenerable and must not be edited, while a scaffolded file is
    // emitted once FOR the user — their edits are the point of the
    // custody, and regeneration never overwrites them (issue #58).
    if (($context['custody'] ?? 'managed') === 'scaffold-once') {
        $lines[] = '// Scaffolded once: you own this file. Edit freely —'
            . ' regeneration never overwrites it while its marker stands.';
    } else {
        $lines[] = '// Do not edit: regenerate with `lekalo generate`.';
    }
    if ($namespace !== '') {
        $lines[] = '';
        $lines[] = 'namespace ' . $namespace . ';';
    }
    return implode("\n", $lines);
}

/** The bare class name of one mapped entry (the final FQN segment). */
function php_types_entry_class(array $entry): string
{
    $cut = strrpos($entry['fqn'], '\\');
    return $cut === false ? $entry['fqn'] : substr($entry['fqn'], $cut + 1);
}

/**
 * Emit every generated file of one mapped type inventory. `input` is
 * `{projectId, mapped, adapterVersion, root}`; returns path-sorted
 * `{path, text}` records with paths under the declared custody root,
 * plus the sidecar document as `map`.
 */
function php_emit_types(array $input): array
{
    $mapped = $input['mapped'];
    $policy = $mapped['policy'];
    $prefix = $policy['namespacePrefix'];
    $definitions = [];
    foreach ($mapped['types'] as $entry) {
        $definitions[$entry['semanticId']] = $entry;
    }
    $context = [
        'adapterVersion' => $input['adapterVersion'],
        'irDigest' => $mapped['digests']['ir'],
        'projectId' => $input['projectId'],
        'custody' => $policy['custody'],
    ];
    $root = $input['root'];

    $files = [];
    $addFile = static function (string $relative, string $text) use (&$files, $root): void {
        $files[] = ['path' => $root . '/' . $relative, 'text' => $text];
    };

    foreach ($mapped['types'] as $entry) {
        $module = php_types_module_of($entry['semanticId']);
        $namespace = $prefix . '\\' . ucfirst($module);
        $description = php_types_entry_description($definitions, $entry['semanticId']);
        switch ($entry['kind']) {
            case 'scalar':
                $addFile($entry['path'], php_types_scalar_text($entry, $context, $namespace, $description));
                break;
            case 'enum':
                $addFile($entry['path'], php_types_enum_text($entry, $context, $namespace, $description));
                break;
            case 'query':
                $addFile($entry['codecPath'], php_types_query_codec_text(
                    $entry,
                    $definitions,
                    $prefix,
                    $context,
                    $namespace,
                    $description,
                ));
                break;
            default:
                $addFile($entry['path'], php_types_structured_text(
                    $entry,
                    $definitions,
                    $prefix,
                    $context,
                    $namespace,
                    $description,
                ));
                $addFile($entry['codecPath'], php_types_codec_text(
                    $entry,
                    $definitions,
                    $prefix,
                    $context,
                    $namespace,
                    $description,
                ));
                break;
        }
    }
    foreach ($mapped['collections'] as $collection) {
        $module = php_types_module_of($collection['element']);
        $namespace = $prefix . '\\' . ucfirst($module);
        $description = php_types_entry_description($definitions, $collection['element']);
        $addFile($collection['path'], php_types_collection_text(
            $collection,
            $definitions,
            $prefix,
            $context,
            $namespace,
            $description,
        ));
    }
    foreach ($mapped['wrappers'] as $wrapper) {
        $module = php_types_module_of($wrapper['element']);
        $namespace = $prefix . '\\' . ucfirst($module) . '\\Optional';
        $description = php_types_entry_description($definitions, $wrapper['element']);
        $addFile($wrapper['path'], php_types_wrapper_text(
            $wrapper,
            $definitions,
            $prefix,
            $context,
            $namespace,
            $description,
        ));
    }
    if ($policy['classMap']) {
        $addFile('classmap.php', php_types_classmap_text($mapped['artifacts'], $context, $prefix));
    }

    $sidecar = php_types_sidecar_document($input['projectId'], $mapped, $context, $root);
    $addFile('types.map.json', php_types_canonical_json($sidecar) . "\n");

    usort($files, static fn (array $left, array $right): int => strcmp($left['path'], $right['path']));
    return ['files' => $files, 'map' => $sidecar];
}

/** The mapped description of one definition, without internal keys. */
function php_types_entry_description(array $definitions, string $semanticId): ?string
{
    $description = $definitions[$semanticId]['description'] ?? null;
    return is_string($description) && $description !== '' ? $description : null;
}

// ---------------------------------------------------------------------------
// Scalar nominal wrappers.
// ---------------------------------------------------------------------------

/**
 * The validation body of one scalar base: a closed, extension-free
 * check that refuses rather than coerces.
 */
function php_types_scalar_validation(string $base): string
{
    switch ($base) {
        case 'string':
            return "        return is_string(\$raw) && \$raw !== '';";
        case 'boolean':
            return '        return is_bool($raw);';
        case 'number':
            return implode("\n", [
                '        if (is_int($raw)) {',
                '            return true;',
                '        }',
                '        // A JSON integer beyond exact float precision arrives as an',
                '        // integral float: refused, never silently rounded.',
                '        return is_float($raw) && is_finite($raw)',
                '            && !($raw === floor($raw) && abs($raw) >= 9007199254740992);',
            ]);
        case 'uuid':
            return implode("\n", [
                "        return is_string(\$raw)",
                "            && preg_match('/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\$/', \$raw) === 1;",
            ]);
        case 'date':
            return implode("\n", [
                "        if (!is_string(\$raw) || preg_match('/^\\\\d{4}-\\\\d{2}-\\\\d{2}\$/', \$raw) !== 1) {",
                '            return false;',
                '        }',
                '        return checkdate((int) substr($raw, 5, 2), (int) substr($raw, 8, 2), (int) substr($raw, 0, 4));',
            ]);
        case 'datetime':
            return implode("\n", [
                '        if (!is_string($raw)',
                "            || preg_match('/^\\\\d{4}-\\\\d{2}-\\\\d{2}T\\\\d{2}:\\\\d{2}:\\\\d{2}(?:\\\\.\\\\d+)?(?:Z|[+-]\\\\d{2}:\\\\d{2})\$/', \$raw) !== 1) {",
                '            return false;',
                '        }',
                '        return checkdate((int) substr($raw, 5, 2), (int) substr($raw, 8, 2), (int) substr($raw, 0, 4));',
            ]);
        case 'uri':
            return implode("\n", [
                "        return is_string(\$raw) && \$raw !== ''",
                "            && preg_match('/^[A-Za-z][A-Za-z0-9+.\\\\-]*:\\\\S*\$/', \$raw) === 1;",
            ]);
        default:
            // The closed base vocabulary is validated upstream; an
            // unknown base here is a kernel bug, not an emission case.
            throw new LogicException('unknown scalar base: ' . $base);
    }
}

/** The emitted text of one nominal scalar wrapper. */
function php_types_scalar_text(array $entry, array $context, string $namespace, ?string $description): string
{
    $base = $entry['base'];
    $class = php_types_entry_class($entry);
    $stringBacked = !in_array($base, ['number', 'boolean'], true);
    $propertyType = $stringBacked ? 'string' : ($base === 'number' ? 'int|float' : 'bool');
    $fromParameter = $stringBacked ? 'string $value' : ($base === 'number' ? 'int|float $value' : 'bool $value');
    $owner = php_types_string_literal($entry['semanticId']);
    $accessor = $stringBacked
        ? implode("\n", [
            '',
            '    /** The exact declared spelling, never a reformatting. */',
            '    public function toString(): string',
            '    {',
            '        return $this->value;',
            '    }',
        ])
        : implode("\n", [
            '',
            '    /** The exact finite value: no numeric-string coercion exists. */',
            '    public function value(): ' . $propertyType,
            '    {',
            '        return $this->value;',
            '    }',
        ]);
    $lines = [
        php_types_file_header($context, $namespace, $entry['semanticId'], $description),
        '',
        'final readonly class ' . $class,
        '{',
        '    private function __construct(',
        '        public readonly ' . $propertyType . ' $value,',
        '    ) {',
        '    }',
        '',
        '    /**',
        '     * Decode one wire value: the declared spelling is validated,',
        '     * never coerced.',
        '     */',
        '    public static function fromWire(mixed $raw): self',
        '    {',
        '        if (!self::isValid($raw)) {',
        '            throw new \\InvalidArgumentException(' . $owner . ' . \': invalid ' . $base . ' wire value\');',
        '        }',
        '        /** @var ' . $propertyType . ' $raw */',
        '        return new self($raw);',
        '    }',
        '',
        '    /** Construct from an already-typed value with the same validation. */',
        '    public static function from(' . $fromParameter . '): self',
        '    {',
        '        if (!self::isValid($value)) {',
        '            throw new \\InvalidArgumentException(' . $owner . ' . \': invalid ' . $base . ' value\');',
        '        }',
        '        return new self($value);',
        '    }',
        '',
        '    private static function isValid(mixed $raw): bool',
        '    {',
        php_types_scalar_validation($base),
        '    }',
        $accessor,
        '',
        '    public function equals(self $other): bool',
        '    {',
        '        return $this->value === $other->value;',
        '    }',
        '}',
    ];
    return implode("\n", $lines) . "\n";
}

// ---------------------------------------------------------------------------
// String-backed enums.
// ---------------------------------------------------------------------------

/** The emitted text of one string-backed enum in declared order. */
function php_types_enum_text(array $entry, array $context, string $namespace, ?string $description): string
{
    $class = php_types_entry_class($entry);
    $owner = php_types_string_literal($entry['semanticId']);
    $lines = [
        php_types_file_header($context, $namespace, $entry['semanticId'], $description),
        '',
        'enum ' . $class . ': string',
        '{',
    ];
    foreach ($entry['values'] as $value) {
        $lines[] = '    case ' . $value['case'] . ' = ' . php_types_string_literal($value['value']) . ';';
    }
    $lines[] = '';
    $lines[] = '    /**';
    $lines[] = '     * Decode one wire value: the declared members are the closed';
    $lines[] = '     * vocabulary; anything else refuses.';
    $lines[] = '     */';
    $lines[] = '    public static function fromWire(mixed $raw): self';
    $lines[] = '    {';
    $lines[] = '        $case = is_string($raw) ? self::tryFrom($raw) : null;';
    $lines[] = '        if ($case === null) {';
    $lines[] = '            throw new \\InvalidArgumentException(' . $owner . ' . \': unknown enum wire value\');';
    $lines[] = '        }';
    $lines[] = '        return $case;';
    $lines[] = '    }';
    $lines[] = '';
    $lines[] = '    /** The exact declared wire value. */';
    $lines[] = '    public function toWire(): string';
    $lines[] = '    {';
    $lines[] = '        return $this->value;';
    $lines[] = '    }';
    $lines[] = '}';
    return implode("\n", $lines) . "\n";
}

// ---------------------------------------------------------------------------
// Immutable collections.
// ---------------------------------------------------------------------------

/** The emitted text of one typed immutable collection class. */
function php_types_collection_text(
    array $collection,
    array $definitions,
    string $prefix,
    array $context,
    string $namespace,
    ?string $description,
): string {
    $elementFqn = '\\' . php_types_leaf_fqn($definitions, $prefix, $collection['element']);
    $nullableElements = $collection['nullableElements'];
    $docType = $nullableElements ? 'list<null|' . $elementFqn . '>' : 'list<' . $elementFqn . '>';
    $elementCheck = $nullableElements
        ? 'if ($item !== null && !$item instanceof ' . $elementFqn . ') {'
        : 'if (!$item instanceof ' . $elementFqn . ') {';
    $elementEquals = $nullableElements
        ? '            if (($item === null) !== ($other->items[$index] === null)'
        . "\n"
        . '                || ($item !== null && !$item->equals($other->items[$index]))) {'
        : '            if (!$item->equals($other->items[$index])) {';
    $owner = php_types_string_literal($collection['element']);
    $lines = [
        php_types_file_header($context, $namespace, $collection['element'], $description),
        '',
        'final readonly class ' . $collection['class'],
        '{',
        '    /** @var ' . $docType . ' */',
        '    private readonly array $items;',
        '',
        '    /** @param ' . $docType . ' $items */',
        '    private function __construct(array $items)',
        '    {',
        '        $this->items = $items;',
        '    }',
        '',
        '    /**',
        '     * Construct from a homogeneous element list: the element type is',
        '     * validated, never trusted, and order is preserved.',
        '     *',
        '     * @param array<array-key, mixed> $items',
        '     */',
        '    public static function fromList(array $items): self',
        '    {',
        '        foreach ($items as $item) {',
        '            ' . $elementCheck,
        '                throw new \\InvalidArgumentException(' . $owner . ' . \': list element type mismatch\');',
        '            }',
        '        }',
        '        /** @var ' . $docType . ' $items */',
        '        return new self(array_values($items));',
        '    }',
        '',
        '    /** @return ' . $docType . ' */',
        '    public function all(): array',
        '    {',
        '        return $this->items;',
        '    }',
        '',
        '    public function count(): int',
        '    {',
        '        return count($this->items);',
        '    }',
        '',
        '    /** The empty list is a value, never a missing one. */',
        '    public function isEmpty(): bool',
        '    {',
        '        return $this->items === [];',
        '    }',
        '',
        '    public function equals(self $other): bool',
        '    {',
        '        if ($this->items === $other->items) {',
        '            return true;',
        '        }',
        '        if (count($this->items) !== count($other->items)) {',
        '            return false;',
        '        }',
        '        foreach ($this->items as $index => $item) {',
        $elementEquals,
        '                return false;',
        '            }',
        '        }',
        '        return true;',
        '    }',
        '}',
    ];
    return implode("\n", $lines) . "\n";
}

// ---------------------------------------------------------------------------
// Concrete presence wrappers.
// ---------------------------------------------------------------------------

/** The emitted text of one concrete presence wrapper. */
function php_types_wrapper_text(
    array $wrapper,
    array $definitions,
    string $prefix,
    array $context,
    string $namespace,
    ?string $description,
): string {
    $acceptsNull = $wrapper['acceptsNull'];
    $class = $wrapper['class'];
    $valueFqn = $wrapper['ofList']
        ? '\\' . $wrapper['collection']
        : '\\' . php_types_leaf_fqn($definitions, $prefix, $wrapper['element']);
    $owner = php_types_string_literal($wrapper['element']);
    $leafDefinition = $definitions[$wrapper['element']];
    $valueEquals = $leafDefinition['kind'] === 'enum'
        ? 'return $this->value === $other->value;'
        : 'return $this->value->equals($other->value);';
    $lines = [
        php_types_file_header($context, $namespace, $wrapper['element'], $description),
        '',
        'final readonly class ' . $class,
        '{',
        '    private const MODE_ABSENT = 0;',
    ];
    if ($acceptsNull) {
        $lines[] = '    private const MODE_NULL = 1;';
    }
    $lines[] = '    private const MODE_VALUE = 2;';
    $lines[] = '';
    $lines[] = '    private function __construct(';
    $lines[] = '        private readonly int $mode,';
    $lines[] = '        private readonly ?' . $valueFqn . ' $value,';
    $lines[] = '    ) {';
    $lines[] = '    }';
    $lines[] = '';
    $lines[] = '    /** The absent state: the member is not on the wire. */';
    $lines[] = '    public static function absent(): self';
    $lines[] = '    {';
    $lines[] = '        return new self(self::MODE_ABSENT, null);';
    $lines[] = '    }';
    if ($acceptsNull) {
        $lines[] = '';
        $lines[] = '    /** The explicit-null state: distinct from absent, never collapsed. */';
        $lines[] = '    public static function ofNull(): self';
        $lines[] = '    {';
        $lines[] = '        return new self(self::MODE_NULL, null);';
        $lines[] = '    }';
    }
    $lines[] = '';
    $lines[] = '    /** The carried-value state. */';
    $lines[] = '    public static function of(' . $valueFqn . ' $value): self';
    $lines[] = '    {';
    $lines[] = '        return new self(self::MODE_VALUE, $value);';
    $lines[] = '    }';
    $lines[] = '';
    $lines[] = '    public function isAbsent(): bool';
    $lines[] = '    {';
    $lines[] = '        return $this->mode === self::MODE_ABSENT;';
    $lines[] = '    }';
    if ($acceptsNull) {
        $lines[] = '';
        $lines[] = '    public function isNull(): bool';
        $lines[] = '    {';
        $lines[] = '        return $this->mode === self::MODE_NULL;';
        $lines[] = '    }';
    }
    $lines[] = '';
    $lines[] = '    /**';
    $lines[] = '     * The carried value; absent (and explicit null) is a';
    $lines[] = '     * LogicException, never a silent default.';
    $lines[] = '     */';
    $lines[] = '    public function get(): ' . $valueFqn;
    $lines[] = '    {';
    $lines[] = '        if ($this->mode !== self::MODE_VALUE || $this->value === null) {';
    $lines[] = '            throw new \\LogicException(' . $owner . ' . \': no value carried\');';
    $lines[] = '        }';
    $lines[] = '        return $this->value;';
    $lines[] = '    }';
    $lines[] = '';
    $lines[] = '    public function equals(self $other): bool';
    $lines[] = '    {';
    $lines[] = '        if ($this->mode !== $other->mode) {';
    $lines[] = '            return false;';
    $lines[] = '        }';
    $lines[] = '        if ($this->mode !== self::MODE_VALUE) {';
    $lines[] = '            return true;';
    $lines[] = '        }';
    $lines[] = '        ' . $valueEquals;
    $lines[] = '    }';
    $lines[] = '}';
    return implode("\n", $lines) . "\n";
}

// ---------------------------------------------------------------------------
// Structured DTOs and value objects.
// ---------------------------------------------------------------------------

/** The equals conjunct of one field: exact, null-aware, never loose. */
function php_types_field_equals(array $field, array $definitions, string $prefix): string
{
    $left = '$this->' . $field['property'];
    $right = '$other->' . $field['property'];
    if (isset($field['type']['wrapper'])) {
        return $left . '->equals(' . $right . ')';
    }
    if ($field['type']['list']) {
        return $left . '->equals(' . $right . ')';
    }
    $definition = $definitions[$field['type']['leaf']];
    if ($definition['kind'] === 'enum') {
        return $left . ' === ' . $right;
    }
    if ($field['nullable']) {
        // Both null (or both identical) or both present and equal.
        return '(' . $left . ' === ' . $right . ')'
            . ' || (' . $left . ' !== null && ' . $right . ' !== null && ' . $left . '->equals(' . $right . '))';
    }
    return $left . '->equals(' . $right . ')';
}

/** The emitted text of one structured DTO/value-object class. */
function php_types_structured_text(
    array $entry,
    array $definitions,
    string $prefix,
    array $context,
    string $namespace,
    ?string $description,
): string {
    $class = php_types_entry_class($entry);
    $lines = [
        php_types_file_header($context, $namespace, $entry['semanticId'], $description),
        '',
        'final readonly class ' . $class,
        '{',
        '    public function __construct(',
    ];
    foreach ($entry['fields'] as $field) {
        $type = php_types_field_php_type($field, $definitions, $prefix);
        $lines[] = '        public readonly ' . $type . ' $' . $field['property'] . ',';
    }
    $lines[] = '    ) {';
    $lines[] = '    }';
    if ($entry['fields'] !== []) {
        $lines[] = '';
        $lines[] = '    /** Exact value equality over the immutable state. */';
        $lines[] = '    public function equals(self $other): bool';
        $lines[] = '    {';
        $conjuncts = array_map(
            static fn (array $field): string => php_types_field_equals($field, $definitions, $prefix),
            $entry['fields'],
        );
        $lines[] = '        return ' . implode("\n            && ", $conjuncts) . ';';
        $lines[] = '    }';
    }
    $lines[] = '}';
    return implode("\n", $lines) . "\n";
}

// ---------------------------------------------------------------------------
// Codecs.
// ---------------------------------------------------------------------------

/** The emitted text of one structured codec class. */
function php_types_codec_text(
    array $entry,
    array $definitions,
    string $prefix,
    array $context,
    string $namespace,
    ?string $description,
): string {
    $class = php_types_entry_class($entry) . 'Codec';
    $typeFqn = '\\' . $entry['fqn'];
    $owner = php_types_string_literal($entry['semanticId']);
    $members = implode(', ', array_map(
        static fn (array $field): string => php_types_string_literal($field['name']),
        $entry['fields'],
    ));
    $decode = [];
    $encode = [];
    $helpers = [];
    foreach ($entry['fields'] as $field) {
        $decode[] = php_types_field_decode($field, $definitions, $prefix, $entry['semanticId']);
        $encode[] = php_types_field_encode($field, $definitions, $prefix);
        if ($field['type']['list']) {
            $helpers[] = php_types_list_helpers($field, $definitions, $prefix, $entry['semanticId']);
        }
    }
    $indent = static function (string $block): string {
        return implode("\n", array_map(
            static fn (string $line): string => $line === '' ? $line : '        ' . $line,
            explode("\n", $block),
        ));
    };
    $constructorArgs = implode(', ', array_map(
        static fn (array $field): string => '$' . $field['property'],
        $entry['fields'],
    ));
    $lines = [
        php_types_file_header($context, $namespace, $entry['semanticId'], $description),
        '',
        'final readonly class ' . $class,
        '{',
        '    private const MEMBERS = [' . $members . '];',
        '',
        '    /**',
        '     * Decode one wire object into the typed value: the member set is',
        '     * closed, presence is exact, and every primitive is validated.',
        '     */',
        '    public static function decode(mixed $wire): ' . $typeFqn,
        '    {',
        '        if (!is_array($wire)) {',
        '            throw new \\InvalidArgumentException(' . $owner . ' . \': wire value is not an object\');',
        '        }',
        '        foreach (array_keys($wire) as $key) {',
        '            if (!in_array($key, self::MEMBERS, true)) {',
        '                throw new \\InvalidArgumentException(' . $owner . ' . \': unknown wire member `\' . $key . \'`\');',
        '            }',
        '        }',
        "\n" . $indent(implode("\n\n", $decode)),
        '        return new ' . $typeFqn . '(' . $constructorArgs . ');',
        '    }',
        '',
        '    /**',
        '     * Encode the typed value back to the wire shape: verbatim member',
        '     * names, absent optionals omitted, explicit nulls preserved.',
        '     */',
        '    public static function encode(' . $typeFqn . ' $value): array',
        '    {',
        '        $result = [];',
        ($encode === [] ? '' : "\n" . $indent(implode("\n", $encode))),
        '        return $result;',
        '    }',
    ];
    foreach ($helpers as $helper) {
        $lines[] = '';
        $lines[] = $helper;
    }
    $lines[] = '}';
    return implode("\n", $lines) . "\n";
}

/** The PHP return type spelling of one query return expression. */
function php_types_query_return_type(array $entry, array $definitions, string $prefix): string
{
    $expr = $entry['returns'];
    if ($expr['list']) {
        $definition = $definitions[$expr['leaf']];
        $class = php_types_stem_of($expr['leaf'], $definition['kind'])
            . ($expr['nullableElements'] ? 'NullableList' : 'List');
        $type = php_types_fqn_of($prefix, php_types_module_of($expr['leaf']), $class);
    } else {
        $type = php_types_leaf_fqn($definitions, $prefix, $expr['leaf']);
    }
    // Inside the codec file the type spells fully qualified, so the
    // relative namespace resolution can never alias it.
    $type = '\\' . $type;
    return $expr['nullable'] ? '?' . $type : $type;
}

/** The emitted text of one query result codec (a direct-body passthrough). */
function php_types_query_codec_text(
    array $entry,
    array $definitions,
    string $prefix,
    array $context,
    string $namespace,
    ?string $description,
): string {
    $class = php_types_entry_class($entry);
    $expr = $entry['returns'];
    $decodeType = php_types_query_return_type($entry, $definitions, $prefix);
    $leaf = $expr['leaf'];
    $definition = $definitions[$leaf];
    if ($expr['list']) {
        $decodeExpr = 'self::decodeResult($wire)';
        $encodeExpr = 'self::encodeResult($value)';
        $helpers = [php_types_query_list_helper($entry, $definitions, $prefix)];
    } else {
        $decodeExpr = php_types_leaf_decode($definitions, $prefix, $leaf, '$wire');
        $encodeExpr = php_types_leaf_encode($definitions, $prefix, $leaf, '$value');
        $helpers = [];
    }
    // The encode return type follows the LEAF kind, not the wire member:
    // enums and string-backed scalars spell string, number spells
    // int|float, boolean spells bool, and a structured leaf delegates to
    // its codec (an array) — never an invented envelope.
    if ($expr['list']) {
        $encodeType = 'array';
    } elseif ($definition['kind'] === 'enum') {
        $encodeType = 'string';
    } elseif ($definition['kind'] === 'scalar') {
        $encodeType = match ($definition['base']) {
            'number' => 'int|float',
            'boolean' => 'bool',
            default => 'string',
        };
    } else {
        $encodeType = 'array';
    }
    $encodeType = $expr['nullable'] ? ('null|' . $encodeType) : $encodeType;
    $decodeTypeNullable = $expr['nullable'] ? '?' . ltrim($decodeType, '?') : $decodeType;
    // A non-nullable output decodes directly: the leaf decode refuses a
    // null wire with its own InvalidArgumentException, never a TypeError
    // from returning null under a declared type.
    $decodeNullGuard = $expr['nullable'] ? '$wire === null ? null : ' : '';
    $encodeNullGuard = $expr['nullable'] ? '$value === null ? null : ' : '';
    $lines = [
        php_types_file_header($context, $namespace, $entry['semanticId'], $description),
        '',
        'final readonly class ' . $class,
        '{',
        '    /**',
        '     * Decode the direct query output: the declared return type is the',
        '     * body; no envelope exists.',
        '     */',
        '    public static function decode(mixed $wire): ' . $decodeTypeNullable,
        '    {',
        '        return ' . $decodeNullGuard . $decodeExpr . ';',
        '    }',
        '',
        '    public static function encode(' . $decodeTypeNullable . ' $value): ' . $encodeType,
        '    {',
        '        return ' . $encodeNullGuard . $encodeExpr . ';',
        '    }',
    ];
    foreach ($helpers as $helper) {
        $lines[] = '';
        $lines[] = $helper;
    }
    $lines[] = '}';
    return implode("\n", $lines) . "\n";
}

/** The list decode/encode helper pair of a list-valued query return. */
function php_types_query_list_helper(array $entry, array $definitions, string $prefix): string
{
    $expr = $entry['returns'];
    $definition = $definitions[$expr['leaf']];
    $class = php_types_stem_of($expr['leaf'], $definition['kind'])
        . ($expr['nullableElements'] ? 'NullableList' : 'List');
    $collection = $prefix . '\\' . ucfirst(php_types_module_of($expr['leaf'])) . '\\' . $class;
    $owner = php_types_string_literal($entry['semanticId']);
    $elementDecode = $expr['nullableElements']
        ? '$element === null ? null : ' . php_types_leaf_decode($definitions, $prefix, $expr['leaf'], '$element')
        : php_types_leaf_decode($definitions, $prefix, $expr['leaf'], '$element');
    $elementEncode = $expr['nullableElements']
        ? '$element === null ? null : ' . php_types_leaf_encode($definitions, $prefix, $expr['leaf'], '$element')
        : php_types_leaf_encode($definitions, $prefix, $expr['leaf'], '$element');

    return implode("\n", [
        '    private static function decodeResult(mixed $raw): \\' . $collection,
        '    {',
        '        if (!is_array($raw) || !array_is_list($raw)) {',
        '            throw new \\InvalidArgumentException(' . $owner . ' . \': query output is not a list\');',
        '        }',
        '        $items = [];',
        '        foreach ($raw as $element) {',
        '            $items[] = ' . $elementDecode . ';',
        '        }',
        '        return \\' . $collection . '::fromList($items);',
        '    }',
        '',
        '    private static function encodeResult(\\' . $collection . ' $value): array',
        '    {',
        '        $items = [];',
        '        foreach ($value->all() as $element) {',
        '            $items[] = ' . $elementEncode . ';',
        '        }',
        '        return $items;',
        '    }',
    ]);
}

// ---------------------------------------------------------------------------
// Class map and sidecar.
// ---------------------------------------------------------------------------

/** The deterministic class map of one generation. */
function php_types_classmap_text(array $artifacts, array $context, string $prefix): string
{
    $entries = [];
    foreach ($artifacts as $artifact) {
        if (in_array($artifact['role'], ['class-map'], true) || $artifact['path'] === 'types.map.json') {
            continue;
        }
        if (str_ends_with($artifact['path'], '.php')) {
            $entries[$artifact['fqn']] = $artifact['path'];
        }
    }
    ksort($entries, SORT_STRING);
    $lines = [
        '<?php',
        '',
        'declare(strict_types=1);',
        '',
        '// Generated by lekalo-target-php-laravel@' . $context['adapterVersion']
            . ' (type generator, issue #58).',
        '// From ' . PHP_TYPES_IR_IDENTITY . ' input ' . $context['irDigest']
            . '. Do not edit: regenerate with `lekalo generate`.',
        '// The deterministic class map of this generation.',
        '',
        'return [',
    ];
    foreach ($entries as $fqn => $path) {
        $lines[] = '    ' . php_types_string_literal($fqn) . ' => __DIR__ . '
            . php_types_string_literal('/' . $path) . ',';
    }
    $lines[] = '];';
    return implode("\n", $lines) . "\n";
}

/**
 * The authoritative mapping sidecar: contract identity, provenance
 * digests, semantic-ID-sorted type entries, and the path-sorted
 * artifact inventory. Internal mapping keys never surface.
 */
function php_types_sidecar_document(string $projectId, array $mapped, array $context, string $root): array
{
    $types = [];
    foreach ($mapped['types'] as $entry) {
        $row = [
            'semanticId' => $entry['semanticId'],
            'kind' => $entry['kind'],
            'fqn' => $entry['fqn'],
            'path' => $entry['path'],
        ];
        if (isset($entry['base'])) {
            $row['base'] = $entry['base'];
        }
        if (isset($entry['values'])) {
            $row['values'] = array_map(
                static fn (array $value): array => ['case' => $value['case'], 'value' => $value['value']],
                $entry['values'],
            );
        }
        if (isset($entry['fields'])) {
            $row['fields'] = array_map(
                static fn (array $field): array => [
                    'name' => $field['name'],
                    'property' => $field['property'],
                    'type' => $field['type'],
                    'presence' => $field['presence'],
                    'nullable' => $field['nullable'],
                ],
                $entry['fields'],
            );
        }
        if (isset($entry['returns'])) {
            $row['returns'] = $entry['returns'];
        }
        $row['codec'] = $entry['codec'];
        $types[] = $row;
    }
    return [
        'schemaVersion' => PHP_TYPES_MAP_SCHEMA_VERSION,
        'identity' => PHP_TYPES_MAP_IDENTITY,
        'adapter' => ['id' => 'lekalo-target-php-laravel', 'version' => $context['adapterVersion']],
        'projectId' => $projectId,
        'custody' => $context['custody'],
        'namespacePrefix' => $mapped['policy']['namespacePrefix'],
        'generatedRoot' => $root,
        'digests' => $mapped['digests'],
        'types' => $types,
        'artifacts' => $mapped['artifacts'],
    ];
}

// ----- bundled compiler module: type-bindings.php -----

/**
 * The checked-type binding join of the PHP generator (issue #58, step
 * 3): a checked custody generation emits NOTHING and instead joins the
 * declared semantic ids against the observed class-shape evidence the
 * scanner published. The join is read-only and strict:
 *
 *   - a missing evidence document, a missing claim, an ambiguous
 *     claim, a stale source digest, and any shape divergence are
 *     typed findings — never conformant, never a rewrite;
 *   - an absent observer can never pass checked acceptance;
 *   - observed evidence alone grants nothing: the declared mapping is
 *     the authority, the evidence only confirms it.
 */

if (!function_exists('php_validate_types_input')) {
    require_once __DIR__ . '/type-policy.php';
}

const PHP_TYPES_BINDING_MISSING = 'php-types.binding-missing';
const PHP_TYPES_BINDING_AMBIGUOUS = 'php-types.binding-ambiguous';
const PHP_TYPES_BINDING_MISMATCH = 'php-types.binding-mismatch';

/**
 * Join one mapped inventory against the parsed evidence document.
 * `evidence` is the parsed `.lekalo/import/observed/types-evidence.json`
 * or null when absent (absence is a finding for every declared id, per
 * the required-evidence rule). Returns wire-shaped findings.
 */
function php_check_type_bindings(array $mapped, ?array $evidence, ?callable $fileDigest): array
{
    $findings = [];
    $records = [];
    // Evidence paths are project-relative: the declared artifact path
    // resolves under the custody root the mapping names.
    $root = $mapped['policy']['custody'] === 'scaffold-once'
        ? (string) $mapped['policy']['scaffoldRoot']
        : PHP_TYPES_GENERATED_ROOT;
    if (is_array($evidence) && ($evidence['schemaVersion'] ?? null) === PHP_TYPES_EVIDENCE_SCHEMA_VERSION
        && ($evidence['identity'] ?? null) === PHP_TYPES_EVIDENCE_IDENTITY
        && is_array($evidence['classes'] ?? null)) {
        foreach ($evidence['classes'] as $record) {
            if (!is_array($record) || !is_string($record['semanticId'] ?? null)) {
                continue;
            }
            $records[$record['semanticId']][] = $record;
        }
    } else {
        // No evidence at all: every declared id is a missing binding.
        // Checked acceptance without an observer is impossible.
        foreach ($mapped['types'] as $entry) {
            $findings[] = [
                'code' => PHP_TYPES_BINDING_MISSING,
                'semanticId' => $entry['semanticId'],
                'detail' => 'no-evidence-document',
            ];
        }
        return php_types_sort_binding_findings($findings);
    }

    foreach ($mapped['types'] as $entry) {
        $id = $entry['semanticId'];
        $claiming = $records[$id] ?? [];
        if ($claiming === []) {
            $findings[] = [
                'code' => PHP_TYPES_BINDING_MISSING,
                'semanticId' => $id,
                'detail' => 'no-observed-class',
            ];
            continue;
        }
        if (count($claiming) > 1) {
            $findings[] = [
                'code' => PHP_TYPES_BINDING_AMBIGUOUS,
                'semanticId' => $id,
                'detail' => 'claimed-by-' . count($claiming) . '-classes',
            ];
            continue;
        }
        $record = $claiming[0];
        $problems = php_types_check_binding_record($entry, $record, $root, $fileDigest);
        foreach ($problems as $problem) {
            $findings[] = [
                'code' => PHP_TYPES_BINDING_MISMATCH,
                'semanticId' => $id,
                'detail' => $problem,
            ];
        }
    }
    return php_types_sort_binding_findings($findings);
}

/**
 * One evidence record against one declared entry: identity, path,
 * freshness, and observed shape.
 *
 * @return list<string> the divergence details (empty = conforms)
 */
function php_types_check_binding_record(array $entry, array $record, string $root, ?callable $fileDigest): array
{
    $problems = [];
    $kind = is_string($record['kind'] ?? null) ? $record['kind'] : null;
    $fqn = is_string($record['fqn'] ?? null) ? $record['fqn'] : null;
    $path = is_string($record['path'] ?? null) ? $record['path'] : null;
    $expectedPath = $root . '/' . $entry['path'];
    if ($kind !== $entry['kind']) {
        $problems[] = 'kind-diverges';
    }
    if ($fqn !== $entry['fqn']) {
        $problems[] = 'fqn-diverges';
    }
    if ($path !== null && $path !== $expectedPath) {
        $problems[] = 'path-diverges';
    }
    // Freshness: the exact observed source bytes must still be on disk.
    // Tampered or regenerated class bytes invalidate the evidence.
    $digest = is_string($record['sourceDigest'] ?? null) ? $record['sourceDigest'] : null;
    if ($digest !== null && $fileDigest !== null) {
        $actual = $fileDigest($path ?? $entry['path']);
        if ($actual === null) {
            $problems[] = 'observed-file-missing';
        } elseif ($actual !== $digest) {
            $problems[] = 'stale-source-digest';
        }
    }
    // Observed shape: enum cases and property spellings must match the
    // declared mapping exactly; extra or missing members diverge.
    if ($entry['kind'] === 'enum' && is_array($record['enumCases'] ?? null)) {
        $declared = [];
        foreach ($entry['values'] as $value) {
            $declared[$value['case']] = $value['value'];
        }
        $observed = [];
        foreach ($record['enumCases'] as $case) {
            if (!is_array($case) || !is_string($case['case'] ?? null) || !is_string($case['value'] ?? null)) {
                $problems[] = 'evidence-shape';
                break;
            }
            $observed[$case['case']] = $case['value'];
        }
        if ($declared !== $observed) {
            $problems[] = 'enum-cases-diverge';
        }
    }
    if (in_array($entry['kind'], ['value-object', 'entity', 'command', 'event'], true)
        && is_array($record['properties'] ?? null)) {
        $declared = [];
        foreach ($entry['fields'] as $field) {
            $declared[$field['property']] = true;
        }
        $observed = [];
        foreach ($record['properties'] as $property) {
            if (!is_array($property) || !is_string($property['name'] ?? null)) {
                $problems[] = 'evidence-shape';
                break;
            }
            $observed[$property['name']] = true;
        }
        if ($declared !== $observed) {
            $problems[] = 'properties-diverge';
        }
    }
    return $problems;
}

/** Binding findings sort deterministically by semantic id, then code. */
function php_types_sort_binding_findings(array $findings): array
{
    usort($findings, static fn (array $left, array $right): int => strcmp(
        $left['semanticId'] . '|' . $left['code'],
        $right['semanticId'] . '|' . $right['code'],
    ));
    return $findings;
}

/**
 * Validate the parsed evidence document shape (closed): returns the
 * document or null. A present-but-invalid document refuses upstream
 * rather than joining over untyped bytes.
 */
function php_validate_types_evidence(mixed $document): ?array
{
    if (!is_array($document)
        || ($document['schemaVersion'] ?? null) !== PHP_TYPES_EVIDENCE_SCHEMA_VERSION
        || ($document['identity'] ?? null) !== PHP_TYPES_EVIDENCE_IDENTITY
        || !is_array($document['classes'] ?? null)) {
        return null;
    }
    foreach ($document['classes'] as $record) {
        if (!is_array($record)
            || !is_string($record['semanticId'] ?? null)
            || !is_string($record['fqn'] ?? null)
            || !is_string($record['path'] ?? null)
            || !is_string($record['sourceDigest'] ?? null)
            || !is_string($record['kind'] ?? null)) {
            return null;
        }
    }
    return $document;
}

// ----- bundled compiler module: operation-policy.php -----

/**
 * The closed operations policy and input validation of the PHP Laravel
 * operations generator (issue #59). Everything here is pure validation
 * over plain data: the bounded operations input document
 * (`lekalo/operations/*.operations.json`, contract
 * `dev.lekalo.php-operations-input@0.4.0`). Nothing reads the
 * filesystem, nothing writes, nothing executes project code.
 *
 * The closed grammar mirrors `contracts/php-operations-input.schema.v0.4.0.json`
 * and the core join (`crates/lekalo-core/src/php_operations/`) exactly:
 * unknown members and unknown enum values refuse. The core join is the
 * acceptance authority; this mirror is the adapter's defensive gate so
 * a request can never reach the emitter half-validated.
 */

const PHP_OPERATIONS_INPUT_SCHEMA_VERSION = 'lekalo/php-operations-input/v0.4.0';
const PHP_OPERATIONS_INPUT_IDENTITY = 'dev.lekalo.php-operations-input@0.4.0';
const PHP_OPERATIONS_MAP_SCHEMA_VERSION = 'lekalo/php-operations-map/v0.4.0';
const PHP_OPERATIONS_MAP_IDENTITY = 'dev.lekalo.php-operations-map@0.4.0';
const PHP_OPERATIONS_EVIDENCE_SCHEMA_VERSION = 'lekalo/php-operations-evidence/v0.4.0';
const PHP_OPERATIONS_EVIDENCE_IDENTITY = 'dev.lekalo.php-operations-evidence@0.4.0';

/** The generated operations root (managed custody). */
const PHP_OPERATIONS_GENERATED_ROOT = '.lekalo/generated/php-laravel/operations';

/** The user-owned scaffold home of operations generation (closed). */
const PHP_OPERATIONS_SCAFFOLD_ROOT = 'app/lekalo-operations';

/** The observed-handler evidence the checked join consumes. */
const PHP_OPERATIONS_EVIDENCE_PATH = '.lekalo/import/observed/operations-evidence.json';

const PHP_OPERATIONS_DEFAULT_NAMESPACE_PREFIX = 'Lekalo\\Generated\\Operations';
const PHP_OPERATIONS_SCAFFOLD_NAMESPACE_PREFIX = 'App\\LekaloOperations';

/** The bounded refusals of the operations input join. */
const PHP_OPERATIONS_REFUSALS = [
    'operations-input-unreadable',
    'operations-input-shape',
    'operations-input-identity',
    'operations-types-unbound',
    'operations-ir-digest',
    'operations-ir-unreadable',
    'operations-ir-shape',
    'operations-ir-identity',
    'operations-input-digest',
];

/**
 * Validate one parsed operations input document against its closed
 * shape. Returns the normalized input array, or null when the document
 * is not the accepted contract (a present-but-invalid document is an
 * authoring error, never an all-defaults fallback).
 */
function php_validate_operations_input(mixed $document): ?array
{
    if (!is_array($document)
        || ($document['schemaVersion'] ?? null) !== PHP_OPERATIONS_INPUT_SCHEMA_VERSION
        || ($document['identity'] ?? null) !== PHP_OPERATIONS_INPUT_IDENTITY) {
        return null;
    }
    // The closed member set (additionalProperties: false): an unknown
    // member is an authoring error, never a silently ignored hint.
    foreach (array_keys($document) as $member) {
        if (!in_array($member, ['schemaVersion', 'identity', 'projectId', 'irDigest', 'typesInputDigest', 'policy', 'operations'], true)) {
            return null;
        }
    }
    $projectId = $document['projectId'] ?? null;
    if (!is_string($projectId) || preg_match('/^[a-z][a-z0-9_]*$/', $projectId) !== 1) {
        return null;
    }
    foreach (['irDigest', 'typesInputDigest'] as $digest) {
        if (!is_sha256_digest($document[$digest] ?? null)) {
            return null;
        }
    }
    $prefix = PHP_OPERATIONS_DEFAULT_NAMESPACE_PREFIX;
    if (array_key_exists('policy', $document)) {
        $policy = $document['policy'];
        if (!is_array($policy) || count($policy) !== 1
            || !isset($policy['namespacePrefix'])
            || !is_string($policy['namespacePrefix'])
            || preg_match('/^[A-Za-z_][A-Za-z0-9_]*(\\\\[A-Za-z_][A-Za-z0-9_]*){1,7}$/', $policy['namespacePrefix']) !== 1) {
            return null;
        }
        $prefix = $policy['namespacePrefix'];
    }
    $operations = $document['operations'] ?? null;
    if (!is_array($operations) || $operations === [] || count($operations) > 4096) {
        return null;
    }
    $records = [];
    $previous = '';
    foreach ($operations as $record) {
        $validated = php_validate_operation_record($record);
        if ($validated === null) {
            return null;
        }
        if ($validated['id'] <= $previous) {
            // Canonical order is part of the shape.
            return null;
        }
        $previous = $validated['id'];
        $records[] = $validated;
    }
    return [
        'projectId' => $projectId,
        'irDigest' => $document['irDigest'],
        'typesInputDigest' => $document['typesInputDigest'],
        'namespacePrefix' => $prefix,
        'operations' => $records,
    ];
}

/**
 * Validate one operation record against the closed shape.
 *
 * @return array<string, mixed>|null
 */
function php_validate_operation_record(mixed $record): ?array
{
    if (!is_array($record)) {
        return null;
    }
    // The closed record member set (additionalProperties: false).
    foreach (array_keys($record) as $member) {
        if (!in_array($member, ['id', 'kind', 'mode', 'entry', 'recipe', 'errors', 'policy', 'transaction'], true)) {
            return null;
        }
    }
    $id = $record['id'] ?? null;
    if (!is_string($id) || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', $id) !== 1) {
        return null;
    }
    $kind = $record['kind'] ?? null;
    if ($kind !== 'command' && $kind !== 'query') {
        return null;
    }
    $mode = $record['mode'] ?? null;
    if (!in_array($mode, ['managed', 'scaffold-once', 'checked', 'custom'], true)) {
        return null;
    }
    $entry = null;
    if (array_key_exists('entry', $record) && $record['entry'] !== null) {
        $entry = php_validate_operation_entry($record['entry']);
        if ($entry === null) {
            return null;
        }
    }
    $recipe = null;
    if (array_key_exists('recipe', $record) && $record['recipe'] !== null) {
        $recipe = php_validate_recipe($record['recipe']);
        if ($recipe === null) {
            return null;
        }
    }
    $errors = [];
    if (array_key_exists('errors', $record) && $record['errors'] !== null) {
        if (!is_array($record['errors']) || count($record['errors']) > 64) {
            return null;
        }
        $previous = '';
        foreach ($record['errors'] as $error) {
            if (!is_string($error)
                || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', $error) !== 1
                || $error <= $previous) {
                return null;
            }
            $previous = $error;
            $errors[] = $error;
        }
    }
    $policyId = null;
    if (array_key_exists('policy', $record) && $record['policy'] !== null) {
        $policy = $record['policy'];
        if (!is_array($policy) || count($policy) !== 1) {
            return null;
        }
        $policyId = $policy['id'] ?? null;
        if (!is_string($policyId)
            || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', $policyId) !== 1) {
            return null;
        }
    }
    $transaction = 'forbidden';
    if (array_key_exists('transaction', $record) && $record['transaction'] !== null) {
        $transaction = $record['transaction']['mode'] ?? null;
        if ($transaction !== 'required' && $transaction !== 'forbidden') {
            return null;
        }
    }
    // Mode pairing: checked/custom declare, managed/scaffold carry the
    // closed recipe that drives the signature (the scaffold body stays
    // the explicit unimplemented failure).
    if (in_array($mode, ['checked', 'custom'], true) !== ($entry !== null)) {
        return null;
    }
    if (in_array($mode, ['managed', 'scaffold-once'], true) !== ($recipe !== null)) {
        return null;
    }
    return [
        'id' => $id,
        'kind' => $kind,
        'mode' => $mode,
        'entry' => $entry,
        'recipe' => $recipe,
        'errors' => $errors,
        'policyId' => $policyId,
        'transaction' => $transaction,
    ];
}

/** Validate one declared native entrypoint. */
function php_validate_operation_entry(mixed $entry): ?array
{
    if (!is_array($entry) || count($entry) !== 3) {
        return null;
    }
    $fqn = $entry['fqn'] ?? null;
    $method = $entry['method'] ?? null;
    $path = $entry['path'] ?? null;
    if (!is_string($fqn)
        || preg_match('/^[A-Za-z_][A-Za-z0-9_]*(\\\\[A-Za-z_][A-Za-z0-9_]*){1,7}$/', $fqn) !== 1) {
        return null;
    }
    if ($method !== 'handle') {
        return null;
    }
    if (!is_string($path) || preg_match('/^[a-z][a-z0-9_.\/-]*\.php$/', $path) !== 1
        || str_contains($path, '..')) {
        return null;
    }
    return ['fqn' => $fqn, 'method' => $method, 'path' => $path];
}

/**
 * Validate one closed recipe. Only the grammar is decided here; the
 * semantic join is the core's authority (and the core runs it before
 * any adapter exchange).
 */
function php_validate_recipe(mixed $recipe): ?array
{
    if (!is_array($recipe) || !is_string($recipe['kind'] ?? null)) {
        return null;
    }
    if ($recipe['kind'] === 'port-delegation') {
        if (count($recipe) > 4) {
            return null;
        }
        $port = $recipe['port'] ?? null;
        $method = $recipe['method'] ?? null;
        $result = $recipe['result'] ?? null;
        if (!is_string($port) || preg_match('/^[A-Z][A-Za-z0-9_]*$/', $port) !== 1) {
            return null;
        }
        if (!is_string($method) || preg_match('/^[a-z][A-Za-z0-9_]*$/', $method) !== 1) {
            return null;
        }
        if ($result !== null
            && (!is_string($result)
                || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', $result) !== 1)) {
            return null;
        }
        return ['kind' => 'port-delegation', 'port' => $port, 'method' => $method, 'result' => $result];
    }
    if ($recipe['kind'] !== 'single-entity-update') {
        return null;
    }
    foreach (['entity', 'key', 'assignments', 'kept', 'missingBehavior'] as $required) {
        if (!array_key_exists($required, $recipe)) {
            return null;
        }
    }
    if (count($recipe) > 8) {
        return null;
    }
    $entity = $recipe['entity'];
    if (!is_string($entity)
        || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', $entity) !== 1) {
        return null;
    }
    $key = $recipe['key'];
    if (!is_string($key) || php_operations_is_field_name($key) !== true) {
        return null;
    }
    $assignments = [];
    if (!is_array($recipe['assignments']) || $recipe['assignments'] === []
        || count($recipe['assignments']) > 64) {
        return null;
    }
    $seen = [];
    foreach ($recipe['assignments'] as $assignment) {
        if (!is_array($assignment) || count($assignment) !== 2
            || !php_operations_is_field_name($assignment['field'] ?? null)) {
            return null;
        }
        $value = php_operations_validate_operand($assignment['value'] ?? null);
        if ($value === null || isset($seen[$assignment['field']])) {
            return null;
        }
        $seen[$assignment['field']] = true;
        $assignments[] = ['field' => $assignment['field'], 'value' => $value];
    }
    $kept = [];
    if (!is_array($recipe['kept']) || count($recipe['kept']) > 64) {
        return null;
    }
    foreach ($recipe['kept'] as $field) {
        if (!php_operations_is_field_name($field) || isset($seen[$field])) {
            return null;
        }
        $seen[$field] = true;
        $kept[] = $field;
    }
    $preconditions = [];
    if (array_key_exists('preconditions', $recipe)) {
        if (!is_array($recipe['preconditions']) || count($recipe['preconditions']) > 16) {
            return null;
        }
        foreach ($recipe['preconditions'] as $precondition) {
            if (!is_array($precondition) || count($precondition) !== 3
                || !php_operations_is_field_name($precondition['field'] ?? null)) {
                return null;
            }
            $equals = php_operations_validate_operand($precondition['equals'] ?? null);
            $error = $precondition['error'] ?? null;
            if ($equals === null || !is_string($error)
                || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', $error) !== 1) {
                return null;
            }
            $preconditions[] = ['field' => $precondition['field'], 'equals' => $equals, 'error' => $error];
        }
    }
    $missing = $recipe['missingBehavior'];
    if (!is_array($missing) || count($missing) !== 1
        || !is_string($missing['error'] ?? null)
        || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', (string) ($missing['error'] ?? '')) !== 1) {
        return null;
    }
    $emissions = [];
    if (array_key_exists('emit', $recipe)) {
        if (!is_array($recipe['emit']) || count($recipe['emit']) > 16) {
            return null;
        }
        foreach ($recipe['emit'] as $emission) {
            if (!is_array($emission) || count($emission) !== 2) {
                return null;
            }
            $event = $emission['event'] ?? null;
            if (!is_string($event)
                || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', $event) !== 1
                || !is_array($emission['payload']) || count($emission['payload']) > 64) {
                return null;
            }
            $payload = [];
            foreach ($emission['payload'] as $field => $operand) {
                $value = php_operations_validate_operand($operand);
                if ($value === null || !php_operations_is_field_name((string) $field)) {
                    return null;
                }
                $payload[$field] = $value;
            }
            $emissions[] = ['event' => $event, 'payload' => $payload];
        }
    }
    return [
        'kind' => 'single-entity-update',
        'entity' => $entity,
        'key' => $key,
        'assignments' => $assignments,
        'kept' => $kept,
        'preconditions' => $preconditions,
        'missingBehavior' => ['error' => $missing['error']],
        'emit' => $emissions,
    ];
}

/** Whether one name is a closed camelCase field identifier. */
function php_operations_is_field_name(mixed $name): bool
{
    return is_string($name)
        && preg_match('/^[a-z][a-zA-Z0-9_]*$/', $name) === 1
        && strlen($name) <= 63;
}

/** One closed typed operand, or null when the shape is foreign. */
function php_operations_validate_operand(mixed $operand): ?array
{
    if (!is_array($operand) || count($operand) !== 1) {
        return null;
    }
    if (isset($operand['fromInput'])) {
        return php_operations_is_field_name($operand['fromInput'])
            ? ['fromInput' => $operand['fromInput']]
            : null;
    }
    if (isset($operand['fromEntity'])) {
        return php_operations_is_field_name($operand['fromEntity'])
            ? ['fromEntity' => $operand['fromEntity']]
            : null;
    }
    if (isset($operand['enumCase'])) {
        $case = $operand['enumCase'];
        if (!is_array($case) || count($case) !== 2) {
            return null;
        }
        $type = $case['type'] ?? null;
        $value = $case['value'] ?? null;
        if (!is_string($type)
            || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', $type) !== 1
            || !is_string($value) || $value === '' || strlen($value) > 64
            || preg_match('/^[a-zA-Z0-9_-]+$/', $value) !== 1) {
            return null;
        }
        return ['enumCase' => ['type' => $type, 'value' => $value]];
    }
    if (array_key_exists('literal', $operand)) {
        $literal = $operand['literal'];
        if (is_string($literal) && strlen($literal) <= 256) {
            return ['literal' => $literal];
        }
        if (is_bool($literal)) {
            return ['literal' => $literal];
        }
        if (is_int($literal)) {
            return ['literal' => $literal];
        }
    }
    return null;
}

// ----- bundled compiler module: operation-map.php -----

/**
 * The operations mapper (issue #59): the validated input plus the
 * compiled IR evidence plus the #58 type-map naming rules become one
 * deterministic operations map — signatures, injected ports, role FQNs
 * and the artifact inventory — before any byte is emitted. Reuses the
 * type-map naming and codec helpers; never re-derives type spellings.
 *
 * Unsupported projections become bounded wire findings with ZERO writes,
 * exactly like the type mapper.
 */

/**
 * Map the validated operations input. `$context` is
 * `{input, ir, definitions, typesPrefix, typesIndex, adapterVersion, irDigest, inputDigest, typesInputDigest, root, namespacePrefix}`.
 * `definitions` is the id-indexed parsed IR; `typesIndex` maps semantic
 * ids to `{fqn, path, codec}` from `php_map_types`.
 */
function php_map_operations(array $context): array
{
    $findings = [];
    $addFinding = static function (string $code, string $semanticId, string $detail) use (&$findings): void {
        $findings[] = ['code' => $code, 'semanticId' => $semanticId, 'detail' => $detail];
    };
    $input = $context['input'];
    $definitions = $context['definitions'];
    $prefix = $context['namespacePrefix'];
    $scaffold = $context['root'] === PHP_OPERATIONS_SCAFFOLD_ROOT;
    $operations = [];
    $files = [];
    /** @var array<string, true> $claimedFqns */
    $claimedFqns = [];
    /** @var array<string, true> $claimedPaths */
    $claimedPaths = [];
    $anyTransaction = false;
    $anyDomainError = false;

    $claim = static function (string $fqn, string $path, string $semanticId) use (&$claimedFqns, &$claimedPaths, $addFinding): bool {
        $fqnKey = strtolower($fqn);
        $pathKey = strtolower($path);
        if (isset($claimedFqns[$fqnKey]) || isset($claimedPaths[$pathKey])) {
            $addFinding('operations.naming-collision', $semanticId, "the name `$fqn` or path `$path` is claimed twice");
            return false;
        }
        $claimedFqns[$fqnKey] = true;
        $claimedPaths[$pathKey] = true;
        return true;
    };

    foreach ($input['operations'] as $record) {
        $mapped = php_operations_map_record($record, $context, $addFinding);
        if ($mapped === null) {
            continue;
        }
        $anyTransaction = $anyTransaction || ($mapped['transaction'] === 'required');
        $anyDomainError = $anyDomainError || ($mapped['errors'] !== []);
        foreach ($mapped['artifacts'] as $artifact) {
            if ($artifact['role'] === 'declared') {
                continue;
            }
            $claim($artifact['fqn'] ?? $artifact['path'], $artifact['path'], $mapped['id']);
        }
        $operations[] = $mapped;
    }
    if ($findings !== []) {
        return ['state' => 'unsupported', 'findings' => php_operations_sort_findings($findings)];
    }

    // The typed error classes: one artifact per unique class, owned by
    // the first sorted declaring operation, emitted once per root.
    $lifecycle = $scaffold ? 'scaffolded' : 'generated';
    $seenErrors = [];
    foreach ($operations as $mapped) {
        foreach ($mapped['errors'] as $error) {
            $key = strtolower((string) $error['fqn']);
            if (isset($seenErrors[$key])) {
                continue;
            }
            $seenErrors[$key] = true;
            $path = $context['root'] . '/' . php_types_snake_of(substr((string) $error['fqn'], strlen($context['namespacePrefix']) + 1)) . '.php';
            if (!$claim((string) $error['fqn'], $path, $mapped['id'])) {
                continue;
            }
            $files[] = [
                'path' => $path,
                'role' => 'errors',
                'fqn' => $error['fqn'],
                'lifecycle' => $lifecycle,
                'operation' => $mapped['id'],
            ];
        }
    }

    // Shared artifacts: deterministic, emitted once, only when used.
    $scaffold = $context['root'] === PHP_OPERATIONS_SCAFFOLD_ROOT;
    $lifecycle = $scaffold ? 'scaffolded' : 'generated';
    $addShared = static function (string $class, string $text, string $role) use ($context, $lifecycle, &$files, $claim): void {
        $fqn = $context['namespacePrefix'] . '\\' . $class;
        $path = php_types_snake_of($class) . '.php';
        if (!$claim($fqn, $path, 'php-operations')) {
            return;
        }
        $files[] = [
            'path' => $context['root'] . '/' . $path,
            'text' => $text,
            'digest' => 'sha256:' . hash('sha256', $text),
            'role' => $role,
            'fqn' => $fqn,
            'lifecycle' => $lifecycle,
        ];
    };
    if ($anyDomainError) {
        $addShared('OperationError', php_operations_operation_error_text($context), 'shared');
    }
    if ($anyTransaction) {
        $addShared('TransactionPort', php_operations_transaction_port_text($context), 'shared');
    }
    $addShared('ActorContext', php_operations_actor_context_text($context), 'shared');

    usort($operations, static fn (array $left, array $right): int => strcmp($left['id'], $right['id']));
    usort($files, static fn (array $left, array $right): int => strcmp($left['path'], $right['path']));
    return [
        'state' => 'mapped',
        'findings' => [],
        'operations' => $operations,
        'files' => $files,
    ];
}

/** Sort findings deterministically by semantic id, then code. */
function php_operations_sort_findings(array $findings): array
{
    usort($findings, static function (array $left, array $right): int {
        return [$left['semanticId'], $left['code']] <=> [$right['semanticId'], $right['code']];
    });
    return $findings;
}

/**
 * Map one operation record: naming, roles, ports and per-operation
 * artifacts. Returns null after recording a finding for any unsupported
 * projection.
 *
 * @param callable(string, string, string): void $addFinding
 * @return array<string, mixed>|null
 */
function php_operations_map_record(array $record, array $context, callable $addFinding): ?array
{
    $id = $record['id'];
    $kind = $record['kind'];
    $mode = $record['mode'];
    $definitions = $context['definitions'];
    $prefix = $context['namespacePrefix'];
    $module = php_types_module_of($id);
    if (in_array(strtolower($module), ['errors', 'optional'], true)) {
        $addFinding('operations.module-reserved', $id, 'module namespace segment collides with the emitted Errors/Optional namespace');
        return null;
    }
    // The semantic join (issue #59): the mirror of the core join's
    // operation-level reconciliation. A record naming a non-definition,
    // a policy that does not apply, an unresolvable effect, an
    // uncovered recipe, or a write recipe without the required
    // transaction binding is a typed finding here — never a silent
    // emission and never a kernel crash.
    if (!php_operations_semantic_join($record, $definitions, $addFinding)) {
        return null;
    }
    $leaf = php_types_leaf_name_of($id);
    $stem = php_types_stem_of($id, 'plain');
    $moduleSegment = ucfirst($module);
    $handlerClass = $stem . 'Handler';
    $handlerFqn = $prefix . '\\' . $moduleSegment . '\\' . $handlerClass;
    $handlerPath = strtolower($module) . '/' . strtolower($leaf) . '/handler.php';
    $entry = ['fqn' => $handlerFqn, 'method' => 'handle', 'path' => $context['root'] . '/' . $handlerPath];

    $inputRole = null;
    $resultRole = null;
    $ports = [];
    $body = null;

    // The input role: commands reuse the #58 command input class;
    // queries get an explicit emitted empty-input DTO.
    if ($kind === 'command') {
        $inputFqn = $context['typesIndex'][$id]['fqn'] ?? null;
        if (!is_string($inputFqn)) {
            $addFinding('operations.type-unresolved', $id, 'the command input type is not part of the mapped type inventory');
            return null;
        }
        $inputRole = ['fqn' => $inputFqn];
    } else {
        $inputClass = $stem . 'Input';
        $inputFqn = $prefix . '\\' . $moduleSegment . '\\' . $inputClass;
        $inputRole = ['fqn' => $inputFqn];
    }

    // The result role: command recipes may declare a ref; queries derive
    // it from the IR returns through the type map — a scalar ref maps to
    // the nominal type, a list ref maps to the mapped collection class
    // of its element entity (issue #50: the planning day lists).
    $recipe = $record['recipe'];
    $returnsRef = $definitions[$id]['returns'] ?? null;
    if ($kind === 'query') {
        if (is_array($returnsRef) && isset($returnsRef['list']['ref'])) {
            $elementRef = $returnsRef['list']['ref'];
            $collection = null;
            foreach ((array) ($context['collections'] ?? []) as $candidate) {
                if (is_array($candidate)
                    && ($candidate['element'] ?? null) === $elementRef
                    && ($candidate['nullableElements'] ?? false) === false) {
                    $collection = $candidate;
                    break;
                }
            }
            if ($collection === null || !is_string($collection['fqn'] ?? null)) {
                $addFinding('operations.type-unresolved', $id, 'the query list element type has no mapped collection class');
                return null;
            }
            $resultRole = ['fqn' => (string) $collection['fqn']];
        } else {
            if (!is_array($returnsRef) || !isset($returnsRef['ref'])) {
                $addFinding('operations.recipe-unsupported', $id, 'a managed query needs a scalar-ref or list-ref returns declaration in v0.4.0');
                return null;
            }
            $resultFqn = $context['typesIndex'][(string) $returnsRef['ref']]['fqn'] ?? null;
            if (!is_string($resultFqn)) {
                $addFinding('operations.type-unresolved', $id, 'the query return type is not part of the mapped type inventory');
                return null;
            }
            $resultRole = ['fqn' => $resultFqn];
        }
    } elseif ($recipe['kind'] === 'port-delegation' && $recipe['result'] !== null) {
        $resultFqn = $context['typesIndex'][$recipe['result']]['fqn'] ?? null;
        if (!is_string($resultFqn)) {
            $addFinding('operations.type-unresolved', $id, 'the declared result type is not part of the mapped type inventory');
            return null;
        }
        $resultRole = ['fqn' => $resultFqn];
    }

    $policyFqn = null;
    if ($kind === 'query' && ($recipe['kind'] ?? '') === 'single-entity-update') {
        // Capability honesty: reads stay reads. The finding vetoes the
        // whole run; no write plan can label a query writable.
        $addFinding('operations.query-write', $id, 'a query can never carry a write recipe; reads stay reads');
        return null;
    }
    if ($record['policyId'] !== null) {
        $policyClass = $stem . 'Policy';
        $policyFqn = $prefix . '\\' . $moduleSegment . '\\' . $policyClass;
        $ports[] = ['name' => $policyClass, 'fqn' => $policyFqn, 'role' => 'policy', 'slot' => php_operations_slot_of($policyClass)];
    }
    $eventsFqn = null;
    $emissions = $recipe['emit'] ?? [];
    if ($emissions !== []) {
        $eventsClass = $stem . 'Events';
        $eventsFqn = $prefix . '\\' . $moduleSegment . '\\' . $eventsClass;
        $ports[] = ['name' => $eventsClass, 'fqn' => $eventsFqn, 'role' => 'events', 'slot' => php_operations_slot_of($eventsClass)];
    }
    $repository = null;
    if (($recipe['kind'] ?? '') === 'single-entity-update') {
        $entityId = (string) $recipe['entity'];
        $entityEntry = $context['typesIndex'][$entityId] ?? null;
        if (!is_array($entityEntry)) {
            $addFinding('operations.type-unresolved', $id, 'the updated entity is not part of the mapped type inventory');
            return null;
        }
        $entityModule = php_types_module_of($entityId);
        $entityStem = php_types_stem_of($entityId, 'plain');
        $repositoryClass = $entityStem . 'Repository';
        $repositoryFqn = $prefix . '\\' . ucfirst($entityModule) . '\\' . $repositoryClass;
        $repositoryPath = strtolower($entityModule) . '/' . php_types_snake_of($repositoryClass) . '.php';
        $repository = [
            'class' => $repositoryClass,
            'fqn' => $repositoryFqn,
            'path' => $repositoryPath,
            'entity' => $entityId,
            'entityFqn' => $entityEntry['fqn'],
            'key' => (string) $recipe['key'],
            'identity' => (string) ($definitions[$entityId]['identity'][0] ?? ''),
        ];
        $ports[] = ['name' => $repositoryClass, 'fqn' => $repositoryFqn, 'role' => 'repository', 'slot' => php_operations_slot_of($repositoryClass)];
    }
    $delegate = null;
    if (($recipe['kind'] ?? '') === 'port-delegation') {
        $delegate = [
            'class' => (string) $recipe['port'],
            'fqn' => $prefix . '\\' . $moduleSegment . '\\' . (string) $recipe['port'],
            'method' => (string) $recipe['method'],
        ];
        $ports[] = ['name' => $delegate['class'], 'fqn' => $delegate['fqn'], 'role' => 'delegation', 'slot' => php_operations_slot_of($delegate['class'])];
    }
    if ($record['transaction'] === 'required') {
        $ports[] = ['name' => 'TransactionPort', 'fqn' => $prefix . '\\TransactionPort', 'role' => 'transactions', 'slot' => 'transactions'];
    }
    usort($ports, static fn (array $left, array $right): int => strcmp($left['name'], $right['name']));

    // Domain error ROLE rows: one per declared error id. The artifact
    // rows are claimed once per unique class by the caller, because the
    // #62 binding sets of sibling operations overlap by design.
    $errors = [];
    foreach ($record['errors'] as $errorId) {
        $errorModule = php_types_module_of($errorId);
        if (in_array(strtolower($errorModule), ['errors', 'optional'], true)) {
            $addFinding('operations.module-reserved', $id, "the error module segment of `$errorId` collides with the Errors namespace");
            return null;
        }
        $errorLeaf = php_types_leaf_name_of($errorId);
        $class = ucfirst(php_operations_pascal_of($errorLeaf)) . 'Error';
        $fqn = $prefix . '\\' . ucfirst($errorModule) . '\\Errors\\' . $class;
        $errors[] = ['id' => $errorId, 'fqn' => $fqn, 'class' => $class];
    }
    usort($errors, static fn (array $left, array $right): int => strcmp($left['id'], $right['id']));

    $scaffold = $context['root'] === PHP_OPERATIONS_SCAFFOLD_ROOT;
    $lifecycle = $scaffold ? 'scaffolded' : 'generated';
    $artifacts = [];
    if ($mode === 'managed' || $mode === 'scaffold-once') {
        $artifacts[] = [
            'path' => $entry['path'],
            'fqn' => $handlerFqn,
            'role' => 'handler',
            'lifecycle' => $lifecycle,
        ];
        if ($kind === 'query') {
            $inputPath = strtolower($module) . '/' . strtolower($leaf) . '/input.php';
            $artifacts[] = [
                'path' => $context['root'] . '/' . $inputPath,
                'fqn' => $inputFqn,
                'role' => 'ports',
                'lifecycle' => $lifecycle,
            ];
        }
        if ($policyFqn !== null) {
            $artifacts[] = [
                'path' => $context['root'] . '/' . strtolower($module) . '/' . strtolower($leaf) . '/policy.php',
                'fqn' => $policyFqn,
                'role' => 'ports',
                'lifecycle' => $lifecycle,
            ];
        }
        if ($eventsFqn !== null) {
            $artifacts[] = [
                'path' => $context['root'] . '/' . strtolower($module) . '/' . strtolower($leaf) . '/events.php',
                'fqn' => $eventsFqn,
                'role' => 'ports',
                'lifecycle' => $lifecycle,
            ];
        }
        if ($delegate !== null) {
            $artifacts[] = [
                'path' => $context['root'] . '/' . strtolower($module) . '/' . strtolower($leaf) . '/delegate.php',
                'fqn' => $delegate['fqn'],
                'role' => 'ports',
                'lifecycle' => $lifecycle,
            ];
        }
        if ($repository !== null) {
            $artifacts[] = [
                'path' => $context['root'] . '/' . $repository['path'],
                'fqn' => $repository['fqn'],
                'role' => 'repository',
                'lifecycle' => $lifecycle,
            ];
        }
    } else {
        // checked/custom: the declared entrypoint is inventoried, never
        // written.
        $declared = $record['entry'];
        $artifacts[] = [
            'path' => strtolower((string) $declared['path']),
            'fqn' => (string) $declared['fqn'],
            'role' => 'declared',
            'lifecycle' => 'checked',
        ];
        $entry = ['fqn' => (string) $declared['fqn'], 'method' => (string) $declared['method'], 'path' => $declared['path']];
    }

    return [
        'id' => $id,
        'kind' => $kind,
        'mode' => $mode,
        'recipe' => $recipe,
        'recipeKind' => is_array($recipe) ? (string) $recipe['kind'] : 'maintained',
        'entry' => $entry,
        'input' => $inputRole,
        'result' => $resultRole,
        'errors' => $errors,
        'ports' => $ports,
        'policyId' => $record['policyId'],
        'transaction' => $record['transaction'],
        'effects' => php_operations_declared_effects($record, $definitions),
        'repository' => $repository,
        'delegate' => $delegate,
        'artifacts' => $artifacts,
    ];
}

/** The injected property slot of one port class name. */
function php_operations_slot_of(string $class): string
{
    return lcfirst($class);
}

/** The Pascal spelling of one snake identifier. */
function php_operations_pascal_of(string $spelling): string
{
    return implode('', array_map(
        static fn (string $part): string => ucfirst($part),
        explode('_', $spelling),
    ));
}

/** The declared IR effect ids of one command record, sorted. */
function php_operations_declared_effects(array $record, array $definitions): array
{
    if ($record['kind'] !== 'command') {
        return [];
    }
    $effects = $definitions[$record['id']]['effects'] ?? [];
    $effects = array_values(array_filter($effects, 'is_string'));
    sort($effects);
    return $effects;
}


// ---------------------------------------------------------------------------
// The semantic join (issue #59): the adapter mirror of the core join's
// operation-level reconciliation over the compiled IR definitions. Both
// authorities agree on the closed vocabulary; a finding here vetoes the
// whole run exactly like a core finding.
// ---------------------------------------------------------------------------

/** The canonical spelling of one closed IR type expression. */
function php_operations_type_spelling(mixed $typeExpr): string
{
    if (!is_array($typeExpr)) {
        return '';
    }
    if (isset($typeExpr['ref']) && is_string($typeExpr['ref'])) {
        return $typeExpr['ref'];
    }
    if (isset($typeExpr['optional'])) {
        return php_operations_type_spelling($typeExpr['optional']) . '?';
    }
    if (isset($typeExpr['list'])) {
        return 'list<' . php_operations_type_spelling($typeExpr['list']) . '>';
    }
    return '';
}

/** The field-index map (`name => type expr`) of one structured definition. */
function php_operations_field_index(array $definition): array
{
    $fields = [];
    foreach ([['fields'], ['input'], ['payload']] as $members) {
        $candidate = $definition;
        foreach ($members as $member) {
            $candidate = $candidate[$member] ?? null;
        }
        if (is_array($candidate)) {
            foreach ($candidate as $field) {
                if (is_array($field) && isset($field['name']) && is_string($field['name'])) {
                    $fields[$field['name']] = $field['type'] ?? null;
                }
            }
            break;
        }
    }
    return $fields;
}

/**
 * The full semantic join of one record. Returns false after recording
 * at least one typed finding; true means the record reconciles with the
 * compiled IR.
 *
 * @param callable(string, string, string): void $addFinding
 */
function php_operations_semantic_join(array $record, array $definitions, callable $addFinding): bool
{
    $id = (string) $record['id'];
    $ok = true;
    $note = static function (string $code, string $detail) use ($addFinding, &$ok, $id): void {
        $ok = false;
        $addFinding($code, $id, $detail);
    };
    $definition = $definitions[$id] ?? null;
    if (!is_array($definition) || ($definition['kind'] ?? null) !== $record['kind']) {
        $note(
            'operations.operation-unresolved',
            "the operation id `$id` is not a compiled IR definition of kind `{$record['kind']}`",
        );
        return false;
    }
    // The transaction binding: a write recipe requires exactly one bound
    // transaction (the body runs inside the TransactionPort run); a
    // query never opens one.
    if ($record['kind'] === 'query' && $record['transaction'] === 'required') {
        $note('operations.transaction-unsupported', 'a query never opens a transaction');
    }
    $recipe = $record['recipe'];
    if (is_array($recipe) && ($recipe['kind'] ?? '') === 'single-entity-update') {
        if ($record['kind'] === 'query') {
            $note('operations.query-write', 'a query can never carry a write recipe; reads stay reads');
            return false;
        }
        if ($record['transaction'] !== 'required') {
            $note(
                'operations.transaction-required',
                'a write recipe requires the required transaction binding: the body runs inside the TransactionPort',
            );
        }
    }
    if (is_array($recipe) && ($recipe['kind'] ?? '') === 'port-delegation'
        && $record['kind'] === 'query' && $recipe['result'] !== null) {
        $note(
            'operations.query-write',
            'a query derives its result from the IR returns; a declared recipe result is redundant',
        );
    }
    // The policy binding: an IR policy definition whose applies_to
    // names this operation.
    if ($record['policyId'] !== null) {
        $policyId = (string) $record['policyId'];
        $policy = $definitions[$policyId] ?? null;
        $applies = is_array($policy)
            && ($policy['kind'] ?? null) === 'policy'
            && in_array($id, is_array($policy['applies_to'] ?? null) ? $policy['applies_to'] : [], true);
        if (!$applies) {
            $note(
                'operations.policy-unresolved',
                "the policy `$policyId` is not a compiled policy applying to this operation",
            );
        }
    }
    // The declared errors: the #62 registry binding equality is the
    // core join's authority (the embedded registry is not staged for
    // the adapter), so the mirror checks what the IR admits: the
    // recipe's failure references must be declared errors.
    if (is_array($recipe) && ($recipe['kind'] ?? '') === 'single-entity-update') {
        php_operations_join_update_recipe($record, $recipe, $definitions, $note);
    }
    return $ok;
}

/**
 * The single-entity-update reconciliation: entity, key typing, full
 * field coverage, typed operands, declared failure references, and the
 * effect-resolved event emissions.
 *
 * @param callable(string, string): void $note
 */
function php_operations_join_update_recipe(array $record, array $recipe, array $definitions, callable $note): void
{
    $id = (string) $record['id'];
    $entityId = (string) $recipe['entity'];
    $entity = $definitions[$entityId] ?? null;
    if (!is_array($entity) || ($entity['kind'] ?? null) !== 'entity') {
        $note('operations.entity-unresolved', "the updated definition `$entityId` is not a compiled entity");
        return;
    }
    $identity = $entity['identity'] ?? [];
    if (!is_array($identity) || count($identity) !== 1) {
        $note('operations.recipe-unsupported', 'v0.4.0 updates only single-identity entities');
        return;
    }
    $entityFields = php_operations_field_index($entity);
    $identityField = (string) $identity[0];
    $inputFields = php_operations_field_index($definitions[$id] ?? []);
    // The key operand types.
    $keyType = $inputFields[(string) $recipe['key']] ?? null;
    $identityType = $entityFields[$identityField] ?? null;
    if ($keyType === null) {
        $note('operations.recipe-coverage', 'the key `' . (string) $recipe['key'] . '` is not a command input field');
    }
    if ($keyType !== null && $identityType !== null
        && php_operations_type_spelling($keyType) !== php_operations_type_spelling($identityType)) {
        $note(
            'operations.type-mismatch',
            'the key input field type `' . php_operations_type_spelling($keyType)
            . '` must equal the identity type `' . php_operations_type_spelling($identityType) . '`',
        );
    }
    // Coverage: every non-identity entity field exactly once, either
    // assigned or kept; the identity carries over.
    $covered = [];
    foreach ($recipe['assignments'] as $assignment) {
        $field = (string) $assignment['field'];
        if (isset($covered[$field])) {
            $note('operations.recipe-coverage', "the field `$field` is covered twice");
            continue;
        }
        $covered[$field] = true;
        if (!isset($entityFields[$field])) {
            $note('operations.recipe-coverage', "the assigned field `$field` is not an entity field");
            continue;
        }
        php_operations_join_operand(
            $assignment['value'],
            $entityFields[$field],
            $inputFields,
            $entityFields,
            $definitions,
            $note,
        );
    }
    foreach ($recipe['kept'] as $field) {
        $field = (string) $field;
        if (isset($covered[$field])) {
            $note('operations.recipe-coverage', "the field `$field` is covered twice");
            continue;
        }
        $covered[$field] = true;
        if (!isset($entityFields[$field])) {
            $note('operations.recipe-coverage', "the kept field `$field` is not an entity field");
        }
    }
    foreach (array_keys($entityFields) as $field) {
        if ($field === $identityField) {
            continue;
        }
        if (!isset($covered[$field])) {
            $note('operations.recipe-coverage', "the entity field `$field` is neither assigned nor kept");
        }
    }
    // Preconditions: entity fields, typed operands, declared errors.
    foreach ($recipe['preconditions'] as $precondition) {
        $field = (string) $precondition['field'];
        if (!isset($entityFields[$field])) {
            $note('operations.recipe-coverage', "the precondition field `$field` is not an entity field");
        } else {
            php_operations_join_operand(
                $precondition['equals'],
                $entityFields[$field],
                $inputFields,
                $entityFields,
                $definitions,
                $note,
            );
        }
        if (!in_array((string) $precondition['error'], $record['errors'], true)) {
            $note(
                'operations.registry-binding',
                'the precondition error `' . (string) $precondition['error'] . '` is not a declared error of this operation',
            );
        }
    }
    if (!in_array((string) $recipe['missingBehavior']['error'], $record['errors'], true)) {
        $note(
            'operations.registry-binding',
            'the missing-record error `' . (string) $recipe['missingBehavior']['error'] . '` is not a declared error of this operation',
        );
    }
    // Emissions: the event must be an IR event emitted by one of the
    // command's effects, and every payload field must carry a typed
    // operand exactly once.
    $emittedEvents = [];
    foreach ($definitions[$id]['effects'] ?? [] as $effectId) {
        $effect = $definitions[(string) $effectId] ?? null;
        if (is_array($effect) && ($effect['kind'] ?? null) === 'effect') {
            foreach ($effect['emits'] ?? [] as $emitted) {
                $emittedEvents[] = (string) $emitted;
            }
        }
    }
    foreach ($recipe['emit'] as $emission) {
        $eventId = (string) $emission['event'];
        if (!in_array($eventId, $emittedEvents, true)) {
            $note(
                'operations.effect-unresolved',
                "the event `$eventId` is not emitted by any effect of this command",
            );
        }
        $event = $definitions[$eventId] ?? null;
        if (!is_array($event) || ($event['kind'] ?? null) !== 'event') {
            $note('operations.type-unresolved', "the definition `$eventId` is not a compiled event");
            continue;
        }
        $eventFields = php_operations_field_index($event);
        foreach (array_keys($eventFields) as $field) {
            if (!isset($emission['payload'][$field])) {
                $note('operations.recipe-coverage', "the event field `$field` has no operand");
            }
        }
        foreach ($emission['payload'] as $field => $operand) {
            if (!isset($eventFields[(string) $field])) {
                $note('operations.recipe-coverage', "the payload operand `$field` is not an event field");
                continue;
            }
            php_operations_join_operand(
                $operand,
                $eventFields[(string) $field],
                $inputFields,
                $entityFields,
                $definitions,
                $note,
            );
        }
    }
}

/**
 * One closed typed operand against one target type expression: exact
 * input/entity field existence and type identity, declared enum cases,
 * and no untyped literal targets.
 *
 * @param array<string, mixed> $inputFields
 * @param array<string, mixed> $entityFields
 * @param callable(string, string): void $note
 */
function php_operations_join_operand(
    array $operand,
    mixed $targetExpr,
    array $inputFields,
    array $entityFields,
    array $definitions,
    callable $note,
): void {
    $target = php_operations_type_spelling($targetExpr);
    if (isset($operand['fromInput'])) {
        $field = (string) $operand['fromInput'];
        if (!isset($inputFields[$field])) {
            $note('operations.type-mismatch', "`$field` is not a command input field");
            return;
        }
        $spelling = php_operations_type_spelling($inputFields[$field]);
        if ($spelling !== $target) {
            $note('operations.type-mismatch', "the input field `$field` carries `$spelling`, the target needs `$target`");
        }
        return;
    }
    if (isset($operand['fromEntity'])) {
        $field = (string) $operand['fromEntity'];
        if (!isset($entityFields[$field])) {
            $note('operations.type-mismatch', "`$field` is not an entity field");
            return;
        }
        $spelling = php_operations_type_spelling($entityFields[$field]);
        if ($spelling !== $target) {
            $note('operations.type-mismatch', "the entity field `$field` carries `$spelling`, the target needs `$target`");
        }
        return;
    }
    if (isset($operand['enumCase'])) {
        $enumId = (string) $operand['enumCase']['type'];
        $value = (string) $operand['enumCase']['value'];
        if ($target !== $enumId) {
            $note('operations.type-mismatch', "the enum case targets `$enumId`, the field carries `$target`");
            return;
        }
        $enum = $definitions[$enumId] ?? null;
        $declared = false;
        if (is_array($enum) && ($enum['kind'] ?? null) === 'enum') {
            foreach ($enum['values'] ?? [] as $candidate) {
                if (is_array($candidate) && (string) $candidate['value'] === $value) {
                    $declared = true;
                    break;
                }
            }
        }
        if (!$declared) {
            $note('operations.type-unresolved', "the enum case value `$value` is not declared by `$enumId`");
        }
        return;
    }
    // A literal operand cannot prove the target definition's shape in
    // v0.4.0: unsupported, exactly like the core join.
    $note('operations.type-mismatch', "a literal operand cannot carry the definition target `$target`");
}

// ----- bundled compiler module: operation-emit.php -----

/**
 * The operations emitter (issue #59): the mapped operations inventory
 * becomes deterministic PHP bytes — handler classes with exactly one
 * public business entrypoint, narrow per-operation ports, typed error
 * classes, shared ActorContext/TransactionPort/OperationError support,
 * the classmap, and the custody sidecar. Pure: no filesystem, no
 * clock, no environment. Identical inputs and pins emit identical
 * bytes across roots.
 */

/** The file header of one emitted operations artifact. */
function php_operations_file_header(array $context, string $namespace, string $semanticId): string
{
    $lines = [
        '<?php',
        '',
        'declare(strict_types=1);',
        '',
        '// Generated by ' . ADAPTER_ID . '@' . ADAPTER_VERSION . ' (operations generator, issue #59).',
        '// From dev.lekalo.ir@0.2.16 input ' . $context['irDigest'] . '.',
        '// Semantic id: ' . $semanticId . '.',
        '// Do not edit: regenerate with `lekalo generate`.',
        '',
        'namespace ' . $namespace . ';',
    ];
    return implode("\n", $lines);
}

/** One operations shared-support class text. */
function php_operations_actor_context_text(array $context): string
{
    $lines = [
        php_operations_file_header($context, $context['namespacePrefix'], 'php-operations/actor-context'),
        '',
        'final readonly class ActorContext',
        '{',
        '    /** @var array<string, string> */',
        '    private array $scopes;',
        '',
        '    /**',
        '     * @param array<string, string> $scopes the declared tenant/owner',
        '     *     dimensions; request payloads never become authority',
        '     */',
        '    public function __construct(',
        '        public readonly string $actorId,',
        '        array $scopes = [],',
        '    ) {',
        '        $this->scopes = $scopes;',
        '    }',
        '',
        '    /** The declared scope dimension, or null when absent. */',
        '    public function scope(string $key): ?string',
        '    {',
        '        return $this->scopes[$key] ?? null;',
        '    }',
        '',
        '    /** @return array<string, string> */',
        '    public function scopes(): array',
        '    {',
        '        return $this->scopes;',
        '    }',
        '',
        '    /** One derived context with one more declared scope dimension. */',
        '    public function with(string $key, string $value): self',
        '    {',
        '        $next = $this->scopes;',
        '        $next[$key] = $value;',
        '        return new self($this->actorId, $next);',
        '    }',
        '}',
    ];
    return implode("\n", $lines) . "\n";
}

/** The shared transaction port text. */
function php_operations_transaction_port_text(array $context): string
{
    $lines = [
        php_operations_file_header($context, $context['namespacePrefix'], 'php-operations/transaction-port'),
        '',
        'interface TransactionPort',
        '{',
        '    /**',
        '     * Run the body inside exactly one transaction of the bound',
        '     * connection: commit on a normal return, roll back and rethrow',
        '     * on any throwable. A typed OperationError thrown inside the',
        '     * body must surface as the rolled-back domain failure, never as',
        '     * a committed half-state.',
        '     */',
        '    public function run(callable $body): mixed;',
        '}',
    ];
    return implode("\n", $lines) . "\n";
}

/** The shared typed-error base class text. */
function php_operations_operation_error_text(array $context): string
{
    $lines = [
        php_operations_file_header($context, $context['namespacePrefix'], 'php-operations/operation-error'),
        '',
        'abstract class OperationError extends \\RuntimeException',
        '{',
        '}',
    ];
    return implode("\n", $lines) . "\n";
}

/**
 * Emit every artifact of one mapped operations inventory. `$input` is
 * `{projectId, mapped, context, root, namespacePrefix, definitions,
 * typesIndex}`. Returns `{files, sidecar}`; every file row carries
 * path/text/digest/role/fqn/lifecycle.
 */
function php_emit_operations(array $input): array
{
    $context = $input['context'];
    $mapped = $input['mapped'];
    $definitions = $input['definitions'];
    $typesIndex = $input['typesIndex'];
    $prefix = $input['namespacePrefix'];
    $root = $input['root'];
    $files = [];
    foreach ($mapped['files'] as $file) {
        if (($file['role'] ?? '') === 'errors' && !isset($file['text'])) {
            // The typed error class: owned by its first declaring
            // operation, in the module's Errors namespace.
            $fqn = (string) $file['fqn'];
            $namespace = substr($fqn, 0, (int) strrpos($fqn, '\\'));
            $text = php_operations_error_text(
                ['id' => (string) ($file['operation'] ?? 'php-operations')],
                ['fqn' => $fqn],
                $namespace,
                $context,
            );
            $files[] = array_merge($file, [
                'text' => $text,
                'digest' => 'sha256:' . hash('sha256', $text),
            ]);
            continue;
        }
        $files[] = $file;
    }
    foreach ($mapped['operations'] as $operation) {
        foreach (php_operations_record_texts($operation, $context, $definitions, $typesIndex, $prefix, $root) as $file) {
            $files[] = $file;
        }
    }
    $classmapText = php_operations_classmap_text($files, $context, $root);
    $files[] = [
        'path' => $root . '/classmap.php',
        'text' => $classmapText,
        'digest' => 'sha256:' . hash('sha256', $classmapText),
        'role' => 'classmap',
        'lifecycle' => $root === PHP_OPERATIONS_SCAFFOLD_ROOT ? 'scaffolded' : 'generated',
    ];
    $sidecarText = php_operations_sidecar_text($input['projectId'], $mapped, $files, $context, $root);
    $files[] = [
        'path' => $root . '/operations.map.json',
        'text' => $sidecarText,
        'digest' => 'sha256:' . hash('sha256', $sidecarText),
        'role' => 'sidecar',
        'lifecycle' => $root === PHP_OPERATIONS_SCAFFOLD_ROOT ? 'scaffolded' : 'generated',
    ];
    usort($files, static fn (array $left, array $right): int => strcmp((string) $left['path'], (string) $right['path']));
    return ['files' => $files];
}

/** Every emitted text of one mapped operation record. */
function php_operations_record_texts(
    array $operation,
    array $context,
    array $definitions,
    array $typesIndex,
    string $prefix,
    string $root,
): array {
    $files = [];
    $lifecycle = $root === PHP_OPERATIONS_SCAFFOLD_ROOT ? 'scaffolded' : 'generated';
    $moduleSegment = ucfirst(php_types_module_of($operation['id']));
    $namespace = $prefix . '\\' . $moduleSegment;
    foreach ($operation['artifacts'] as $artifact) {
        if ($artifact['role'] === 'declared') {
            continue;
        }
        $artifactNamespace = $artifact['role'] === 'errors'
            ? $prefix . '\\' . $moduleSegment . '\\Errors'
            : $namespace;
        $text = match ($artifact['role']) {
            'handler' => php_operations_handler_text($operation, $context, $definitions, $typesIndex, $namespace, $artifact),
            'ports' => php_operations_port_text($operation, $artifact, $definitions, $typesIndex, $artifactNamespace, $context),
            'repository' => php_operations_repository_text($operation, $artifact, $definitions, $typesIndex, $namespace, $context),
            'errors' => php_operations_error_text($operation, $artifact, $artifactNamespace, $context),
            default => null,
        };
        if ($text === null) {
            continue;
        }
        $files[] = [
            'path' => $artifact['path'],
            'text' => $text,
            'digest' => 'sha256:' . hash('sha256', $text),
            'role' => $artifact['role'],
            'fqn' => $artifact['fqn'],
            'lifecycle' => $lifecycle,
            'operation' => $operation['id'],
        ];
    }
    return $files;
}

/** The handler class text of one mapped record. */
function php_operations_handler_text(
    array $operation,
    array $context,
    array $definitions,
    array $typesIndex,
    string $namespace,
    array $artifact,
): string {
    $actor = '\\' . $context['namespacePrefix'] . '\\ActorContext';
    $inputFqn = '\\' . $operation['input']['fqn'];
    $returnType = $operation['result'] === null ? 'void' : '\\' . $operation['result']['fqn'];
    $lines = [
        php_operations_file_header($context, $namespace, $operation['id']),
        '',
        'final readonly class ' . php_types_entry_class(['fqn' => (string) $artifact['fqn']]),
        '{',
        '    public function __construct(',
    ];
    foreach ($operation['ports'] as $port) {
        $lines[] = '        private readonly \\' . $port['fqn'] . ' $' . $port['slot'] . ',';
    }
    $lines[] = '    ) {';
    $lines[] = '    }';
    $lines[] = '';
    $lines[] = '    /** The one public business entrypoint of this operation. */';
    $lines[] = '    public function handle(' . $inputFqn . ' $input, ' . $actor . ' $actor): ' . $returnType;
    $lines[] = '    {';
    foreach (php_operations_body_lines($operation, $definitions, $typesIndex, $context) as $line) {
        $lines[] = $line;
    }
    $lines[] = '    }';
    $lines[] = '}';
    return implode("\n", $lines) . "\n";
}

/** The indented body lines of one handler. */
function php_operations_body_lines(array $operation, array $definitions, array $typesIndex, array $context): array
{
    $recipe = $operation['recipe'];
    if ($operation['mode'] === 'scaffold-once') {
        return [
            '        // The scaffold is the explicit unimplemented failure: the',
            '        // signature and dependencies are generated, the body is',
            '        // maintained from here on (issue #59).',
            "        throw new \\LogicException('" . $operation['id'] . ": the maintained operation body is not implemented yet (issue #59).');",
        ];
    }
    if (($recipe['kind'] ?? '') === 'port-delegation') {
        $delegateSlot = php_operations_slot_of((string) $recipe['port']);
        $call = '$this->' . $delegateSlot . '->' . (string) $recipe['method'] . '($input, $actor)';
        if (($operation['transaction'] ?? 'forbidden') === 'required') {
            // The required transaction binding wraps the one maintained
            // delegation: rollback on a typed failure is the port's own
            // guarantee, committed atomically with its writes (issue #50).
            $unit = $operation['result'] === null;
            return [
                '        ' . ($unit ? '' : 'return ') . '$this->transactions->run(function () use ($input, $actor)' . ($unit ? ': void' : ': mixed') . ' {',
                '            ' . ($unit ? '' : 'return ') . $call . ';',
                '        });',
            ];
        }
        if ($operation['result'] === null) {
            return ['        ' . $call . ';'];
        }
        return ['        return ' . $call . ';'];
    }
    // The bounded single-entity-update recipe.
    $entityId = (string) $recipe['entity'];
    $entityVar = '$' . lcfirst(php_types_stem_of($entityId, 'plain'));
    $repositorySlot = php_operations_slot_of(php_types_stem_of($entityId, 'plain') . 'Repository');
    $body = [];
    $unit = $operation['result'] === null;
    $body[] = '        ' . ($unit ? '' : 'return ') . '$this->transactions->run(function () use ($input, $actor)' . ($unit ? ': void' : ': mixed') . ' {';
    if ($operation['policyId'] !== null) {
        $body[] = '            $this->' . php_operations_slot_of(php_types_stem_of($operation['id'], 'plain') . 'Policy')
            . '->authorize($input, $actor);';
    }
    $body[] = '            ' . $entityVar . ' = $this->' . $repositorySlot . '->find($input->'
        . php_types_property_of((string) $recipe['key']) . ');';
    $body[] = '            if (' . $entityVar . ' === null) {';
    $body[] = '                throw new ' . php_operations_error_class_of((string) $recipe['missingBehavior']['error'], $operation) . '();';
    $body[] = '            }';
    foreach ($recipe['preconditions'] as $precondition) {
        $body[] = '            ' . php_operations_precondition_statement(
            $precondition,
            $recipe,
            $definitions,
            $typesIndex,
            $entityVar,
            $operation,
            $context,
        );
        $body[] = '                throw new ' . php_operations_error_class_of((string) $precondition['error'], $operation) . '();';
        $body[] = '            }';
    }
    $body[] = '            ' . $entityVar . 'Updated = ' . php_operations_rebuild_expr($recipe, $definitions, $typesIndex, $entityVar, $operation, $context) . ';';
    $body[] = '            $this->' . $repositorySlot . '->save(' . $entityVar . 'Updated);';
    foreach ($recipe['emit'] as $emission) {
        $slot = php_operations_slot_of(php_types_stem_of($operation['id'], 'plain') . 'Events');
        $method = lcfirst(php_types_stem_of((string) $emission['event'], 'plain'));
        $body[] = '            $this->' . $slot . '->' . $method . '('
            . php_operations_event_expr($emission, $recipe, $definitions, $typesIndex, $entityVar, $operation, $context) . ');';
    }
    if (!$unit) {
        $body[] = '            return null;';
    }
    $body[] = '        });';
    return $body;
}

/**
 * The fully-qualified spelling of one declared error id within one
 * record: the typed error classes live in the module's `Errors`
 * namespace, so the handler body must qualify them — an unqualified
 * reference inside the operation namespace would resolve to a class
 * that does not exist.
 */
function php_operations_error_class_of(string $errorId, array $operation): string
{
    foreach ($operation['errors'] as $error) {
        if ($error['id'] === $errorId) {
            return '\\' . (string) $error['fqn'];
        }
    }
    // The join guarantees membership; this is a kernel bug guard.
    throw new LogicException('undeclared error: ' . $errorId);
}

/** The leaf ref id of one closed IR type expression, or null. */
function php_operations_leaf_ref(mixed $typeExpr): ?string
{
    if (is_array($typeExpr) && isset($typeExpr['ref'])) {
        return (string) $typeExpr['ref'];
    }
    if (is_array($typeExpr) && isset($typeExpr['optional'])) {
        return php_operations_leaf_ref($typeExpr['optional']);
    }
    if (is_array($typeExpr) && isset($typeExpr['list'])) {
        return php_operations_leaf_ref($typeExpr['list']);
    }
    return null;
}

/** One typed comparison statement for one precondition. */
function php_operations_precondition_statement(
    array $precondition,
    array $recipe,
    array $definitions,
    array $typesIndex,
    string $entityVar,
    array $operation,
    array $context,
): string {
    $field = (string) $precondition['field'];
    $targetExpr = php_operations_entity_field_expr((string) $recipe['entity'], $field, $definitions);
    $leaf = php_operations_leaf_ref($targetExpr);
    $kind = $leaf !== null ? (string) ($definitions[$leaf]['kind'] ?? '') : '';
    $actual = $entityVar . '->' . php_types_property_of($field);
    $expected = php_operations_operand_expr(
        $precondition['equals'],
        $targetExpr,
        $definitions,
        $typesIndex,
        $recipe,
        $operation,
        $context,
    );
    $compare = $kind === 'enum'
        ? $actual . ' !== ' . $expected
        : '!' . $actual . '->equals(' . $expected . ')';
    return 'if (' . $compare . ') {';
}

/** The assignment/rebuild expression of the update recipe. */
function php_operations_rebuild_expr(
    array $recipe,
    array $definitions,
    array $typesIndex,
    string $entityVar,
    array $operation,
    array $context,
): string {
    $entityId = (string) $recipe['entity'];
    $entityFqn = (string) $typesIndex[$entityId]['fqn'];
    $args = [];
    foreach ($definitions[$entityId]['fields'] as $field) {
        $name = (string) $field['name'];
        $handled = false;
        foreach ($recipe['assignments'] as $assignment) {
            if ($assignment['field'] === $name) {
                $args[] = php_operations_operand_expr(
                    $assignment['value'],
                    $field['type'],
                    $definitions,
                    $typesIndex,
                    $recipe,
                    $operation,
                    $context,
                );
                $handled = true;
                break;
            }
        }
        if ($handled) {
            continue;
        }
        // The identity and every kept field carry over from the read.
        $args[] = $entityVar . '->' . php_types_property_of($name);
    }
    return 'new \\' . $entityFqn . '(' . implode(', ', $args) . ')';
}

/** One event construction expression. */
function php_operations_event_expr(
    array $emission,
    array $recipe,
    array $definitions,
    array $typesIndex,
    string $entityVar,
    array $operation,
    array $context,
): string {
    $eventId = (string) $emission['event'];
    $args = [];
    foreach ($definitions[$eventId]['payload'] as $field) {
        $name = (string) $field['name'];
        $args[] = php_operations_operand_expr(
            $emission['payload'][$name],
            $field['type'],
            $definitions,
            $typesIndex,
            $recipe,
            $operation,
            $context,
        );
    }
    return 'new \\' . (string) $typesIndex[$eventId]['fqn'] . '(' . implode(', ', $args) . ')';
}

/**
 * One closed operand expression. `$targetExpr` is the target field's
 * IR type expression; it types literal operands exactly.
 */
function php_operations_operand_expr(
    array $operand,
    mixed $targetExpr,
    array $definitions,
    array $typesIndex,
    array $recipe,
    array $operation,
    array $context,
): string {
    if (isset($operand['fromInput'])) {
        return '$input->' . php_types_property_of((string) $operand['fromInput']);
    }
    if (isset($operand['fromEntity'])) {
        $entityVar = '$' . lcfirst(php_types_stem_of((string) $recipe['entity'], 'plain'));
        return $entityVar . '->' . php_types_property_of((string) $operand['fromEntity']);
    }
    if (isset($operand['enumCase'])) {
        $enumId = (string) $operand['enumCase']['type'];
        $value = (string) $operand['enumCase']['value'];
        return '\\' . $typesIndex[$enumId]['fqn'] . '::' . php_operations_enum_case_of($definitions[$enumId], $value);
    }
    // A literal wraps in the exact scalar wrapper of the target leaf.
    $leaf = php_operations_leaf_ref($targetExpr);
    $fqn = $leaf !== null ? ($typesIndex[$leaf]['fqn'] ?? null) : null;
    if (is_string($fqn)) {
        $literal = $operand['literal'];
        $spelling = is_string($literal) ? php_types_string_literal($literal) : var_export($literal, true);
        return '\\' . $fqn . '::from(' . $spelling . ')';
    }
    // The join guarantees a typed scalar target; kernel bug guard.
    throw new LogicException('literal operand without a typed scalar target');
}

/** The declared case name of one enum value, derived exactly like the type map. */
function php_operations_enum_case_of(array $definition, string $value): string
{
    foreach ($definition['values'] as $candidate) {
        if ((string) $candidate['value'] === $value) {
            return ucfirst((string) preg_replace('/[^a-zA-Z0-9]/', '_', $value));
        }
    }
    throw new LogicException('undeclared enum value: ' . $value);
}

/** The type expression of one entity field. */
function php_operations_entity_field_expr(string $entityId, string $field, array $definitions): mixed
{
    foreach ($definitions[$entityId]['fields'] as $candidate) {
        if ((string) $candidate['name'] === $field) {
            return $candidate['type'];
        }
    }
    throw new LogicException('unknown entity field: ' . $field);
}

/** One narrow port interface (or query input DTO) text. */
function php_operations_port_text(
    array $operation,
    array $artifact,
    array $definitions,
    array $typesIndex,
    string $namespace,
    array $context,
): string {
    $actor = '\\' . $context['namespacePrefix'] . '\\ActorContext';
    $inputFqn = '\\' . $operation['input']['fqn'];
    $class = php_types_entry_class(['fqn' => (string) $artifact['fqn']]);
    // The query input DTO: an explicit empty input, never a silent null.
    if ($operation['kind'] === 'query' && str_ends_with((string) $artifact['fqn'], 'Input')) {
        $lines = [
            php_operations_file_header($context, $namespace, $operation['id']),
            '',
            'final readonly class ' . $class,
            '{',
            '    public function __construct()',
            '    {',
            '    }',
            '}',
        ];
        return implode("\n", $lines) . "\n";
    }
    $role = null;
    foreach ($operation['ports'] as $port) {
        if ($port['fqn'] === $artifact['fqn']) {
            $role = $port['role'];
        }
    }
    $lines = [
        php_operations_file_header($context, $namespace, $operation['id']),
        '',
        'interface ' . $class,
        '{',
    ];
    if ($role === 'policy') {
        $lines[] = '    /** The declared authorization gate; a denial throws one typed error. */';
        $lines[] = '    public function authorize(' . $inputFqn . ' $input, ' . $actor . ' $actor): void;';
    } elseif ($role === 'events') {
        foreach ($operation['recipe']['emit'] ?? [] as $emission) {
            $eventFqn = '\\' . $typesIndex[(string) $emission['event']]['fqn'];
            $method = lcfirst(php_types_stem_of((string) $emission['event'], 'plain'));
            $lines[] = '    public function ' . $method . '(' . $eventFqn . ' $event): void;';
        }
    } elseif ($role === 'delegation') {
        $returnType = $operation['result'] === null ? 'void' : '\\' . $operation['result']['fqn'];
        $method = (string) $operation['recipe']['method'];
        $lines[] = '    /** The one maintained body this managed wrapper delegates to. */';
        $lines[] = '    public function ' . $method . '(' . $inputFqn . ' $input, ' . $actor . ' $actor): ' . $returnType . ';';
    }
    $lines[] = '}';
    return implode("\n", $lines) . "\n";
}

/** One narrow repository interface text. */
function php_operations_repository_text(
    array $operation,
    array $artifact,
    array $definitions,
    array $typesIndex,
    string $namespace,
    array $context,
): string {
    $repository = $operation['repository'];
    $entityFqn = '\\' . $repository['entityFqn'];
    $keyId = php_operations_leaf_ref(
        php_operations_entity_field_expr((string) $repository['entity'], (string) $repository['identity'], $definitions),
    );
    $keyFqn = '\\' . $typesIndex[(string) $keyId]['fqn'];
    $lines = [
        php_operations_file_header($context, $namespace, $operation['id']),
        '',
        'interface ' . php_types_entry_class(['fqn' => (string) $artifact['fqn']]),
        '{',
        '    public function find(' . $keyFqn . ' $key): ?' . $entityFqn . ';',
        '',
        '    public function save(' . $entityFqn . ' $entity): void;',
        '}',
    ];
    return implode("\n", $lines) . "\n";
}

/** One typed error class text. */
function php_operations_error_text(
    array $operation,
    array $artifact,
    string $namespace,
    array $context,
): string {
    $lines = [
        php_operations_file_header($context, $namespace, $operation['id']),
        '',
        'final class ' . php_types_entry_class(['fqn' => (string) $artifact['fqn']]),
        '    extends \\' . $context['namespacePrefix'] . '\\OperationError',
        '{',
        '}',
    ];
    return implode("\n", $lines) . "\n";
}

/** The deterministic classmap text over the collected files. */
function php_operations_classmap_text(array $files, array $context, string $root): string
{
    $entries = [];
    foreach ($files as $file) {
        if (!isset($file['fqn']) || $file['role'] === 'sidecar') {
            continue;
        }
        $relative = substr((string) $file['path'], strlen($root) + 1);
        $entries[(string) $file['fqn']] = $relative;
    }
    ksort($entries);
    $lines = [
        '<?php',
        '',
        'declare(strict_types=1);',
        '',
        '// Generated by ' . ADAPTER_ID . '@' . ADAPTER_VERSION . ' (operations generator, issue #59).',
        '// The deterministic class map: fully-qualified name => root-relative path.',
        '// The loading authority of the generated operations tree - no runtime',
        '// registration magic, no Composer scan.',
        '',
        'return [',
    ];
    foreach ($entries as $fqn => $relative) {
        $lines[] = '    ' . php_types_string_literal($fqn) . ' => ' . php_types_string_literal($relative) . ',';
    }
    $lines[] = '];';
    return implode("\n", $lines) . "\n";
}

/** The canonical sidecar bytes over the complete inventory. */
function php_operations_sidecar_text(
    string $projectId,
    array $mapped,
    array $files,
    array $context,
    string $root,
): string {
    $operations = [];
    foreach ($mapped['operations'] as $operation) {
        $entry = [
            'id' => $operation['id'],
            'kind' => $operation['kind'],
            'mode' => $operation['mode'],
        ];
        if ($operation['mode'] === 'managed') {
            $entry['recipe'] = $operation['recipeKind'];
        }
        $entry['entry'] = [
            'fqn' => $operation['entry']['fqn'],
            'method' => $operation['entry']['method'],
            'path' => $operation['entry']['path'],
        ];
        $entry['input'] = ['fqn' => $operation['input']['fqn']];
        if ($operation['result'] !== null) {
            $entry['result'] = ['fqn' => $operation['result']['fqn']];
        }
        if ($operation['errors'] !== []) {
            $entry['errors'] = array_map(
                static fn (array $error): array => ['id' => $error['id'], 'fqn' => $error['fqn']],
                $operation['errors'],
            );
        }
        if ($operation['ports'] !== []) {
            $entry['ports'] = array_map(
                static fn (array $port): array => array_filter([
                    'name' => $port['name'],
                    'role' => $port['role'],
                    'entity' => $port['entity'] ?? null,
                ], static fn (mixed $value): bool => $value !== null),
                $operation['ports'],
            );
        }
        if ($operation['policyId'] !== null) {
            $entry['policy'] = $operation['policyId'];
        }
        $entry['transaction'] = $operation['transaction'];
        $entry['effects'] = $operation['effects'];
        $operations[] = $entry;
    }
    $artifacts = [];
    foreach ($mapped['operations'] as $operation) {
        foreach ($operation['artifacts'] as $artifact) {
            if ($artifact['role'] !== 'declared') {
                continue;
            }
            $artifacts[] = [
                'path' => $artifact['path'],
                'role' => 'declared',
                'lifecycle' => 'checked',
            ];
        }
    }
    foreach ($files as $file) {
        $row = [
            'path' => substr((string) $file['path'], strlen($root) + 1),
            'role' => $file['role'],
            'lifecycle' => $file['lifecycle'],
        ];
        if (isset($file['operation'])) {
            $row['operation'] = $file['operation'];
        }
        if (isset($file['digest'])) {
            $row['digest'] = $file['digest'];
        }
        $artifacts[] = $row;
    }
    usort($artifacts, static fn (array $left, array $right): int => strcmp((string) $left['path'], (string) $right['path']));
    $document = [
        'schemaVersion' => PHP_OPERATIONS_MAP_SCHEMA_VERSION,
        'identity' => PHP_OPERATIONS_MAP_IDENTITY,
        'projectId' => $projectId,
        'adapter' => [
            'id' => ADAPTER_ID,
            'version' => ADAPTER_VERSION,
            'digest' => sha256_digest(ADAPTER_ID . '@' . ADAPTER_VERSION),
        ],
        'digests' => [
            'ir' => $context['irDigest'],
            'input' => $context['inputDigest'],
            'typesInput' => $context['typesInputDigest'],
        ],
        'operations' => $operations,
        'artifacts' => $artifacts,
    ];
    return php_types_canonical_json($document) . "\n";
}

// ----- bundled compiler module: operation-bindings.php -----

/**
 * The operations bindings join (issue #59): declared checked and custom
 * entrypoints join against the observed-handler evidence document. The
 * architecture mirrors the #58 type bindings join: a closed evidence
 * shape, exact identity digests, and typed findings for missing,
 * ambiguous, stale, or diverging records. The join compares the
 * declared FQN/method/path and the observed constructor slots, method
 * shape, and public entrypoint count. Presence of a plausible document
 * is never trusted as producer execution — the producer receipt rides
 * the evidence, and the pinned-tool lane owns its authenticity.
 */

const PHP_OPERATIONS_BINDING_MISSING = 'operations.binding-missing';
const PHP_OPERATIONS_BINDING_AMBIGUOUS = 'operations.binding-ambiguous';
const PHP_OPERATIONS_BINDING_MISMATCH = 'operations.binding-mismatch';
const PHP_OPERATIONS_BINDING_STALE = 'operations.binding-stale';

/**
 * Validate one parsed observed evidence document against its closed
 * shape. Returns the normalized document or null.
 *
 * @return array<string, mixed>|null
 */
function php_validate_operations_evidence(mixed $document): ?array
{
    if (!is_array($document)
        || ($document['schemaVersion'] ?? null) !== PHP_OPERATIONS_EVIDENCE_SCHEMA_VERSION
        || ($document['identity'] ?? null) !== PHP_OPERATIONS_EVIDENCE_IDENTITY) {
        return null;
    }
    $projectId = $document['projectId'] ?? null;
    if (!is_string($projectId) || preg_match('/^[a-z][a-z0-9_]*$/', $projectId) !== 1) {
        return null;
    }
    foreach (['irDigest', 'operationsInputDigest'] as $digest) {
        if (!is_sha256_digest($document[$digest] ?? null)) {
            return null;
        }
    }
    $producer = $document['producer'] ?? null;
    if (!is_array($producer)
        || ($producer['tool'] ?? null) !== 'mago'
        || !is_string($producer['version'] ?? null)
        || preg_match('/^[0-9]+\.[0-9]+\.[0-9]+$/', (string) ($producer['version'] ?? '')) !== 1) {
        return null;
    }
    if (array_key_exists('receiptPath', $producer)) {
        if (!is_string($producer['receiptPath'])
            || preg_match('/^[.a-z][a-z0-9_\/.-]*\.json$/', $producer['receiptPath']) !== 1) {
            return null;
        }
        if (isset($producer['receiptDigest']) && !is_sha256_digest($producer['receiptDigest'])) {
            return null;
        }
    }
    $sources = $document['sources'] ?? null;
    if (!is_array($sources) || $sources === [] || count($sources) > 4096) {
        return null;
    }
    $byPath = [];
    $previous = '';
    foreach ($sources as $source) {
        if (!is_array($source) || count($source) !== 2
            || !is_string($source['path'] ?? null)
            || preg_match('/^[a-z][a-z0-9_\/.-]*\.php$/', (string) ($source['path'] ?? '')) !== 1
            || !is_sha256_digest($source['digest'] ?? null)) {
            return null;
        }
        if ($source['path'] <= $previous) {
            return null;
        }
        $previous = $source['path'];
        $byPath[$source['path']] = $source['digest'];
    }
    $operations = $document['operations'] ?? null;
    if (!is_array($operations) || $operations === [] || count($operations) > 4096) {
        return null;
    }
    $records = [];
    $previousId = '';
    foreach ($operations as $record) {
        $validated = php_operations_validate_evidence_record($record);
        if ($validated === null) {
            return null;
        }
        if ($validated['id'] <= $previousId) {
            return null;
        }
        $previousId = $validated['id'];
        if (!isset($byPath[strtolower((string) $validated['path'])])) {
            // Every record's source must appear in the inventory with
            // the identical digest, or the document is not a closed
            // observation.
            return null;
        }
        $records[] = $validated;
    }
    return [
        'projectId' => $projectId,
        'irDigest' => $document['irDigest'],
        'operationsInputDigest' => $document['operationsInputDigest'],
        'producer' => $producer,
        'sources' => $byPath,
        'operations' => $records,
    ];
}

/** One observed operation record against the closed shape. */
function php_operations_validate_evidence_record(mixed $record): ?array
{
    if (!is_array($record) || count($record) !== 9) {
        return null;
    }
    $id = $record['id'] ?? null;
    if (!is_string($id) || preg_match('/^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$/', $id) !== 1) {
        return null;
    }
    $fqn = $record['fqn'] ?? null;
    if (!is_string($fqn)
        || preg_match('/^[A-Za-z_][A-Za-z0-9_]*(\\\\[A-Za-z_][A-Za-z0-9_]*){0,7}$/', $fqn) !== 1) {
        return null;
    }
    if (($record['method'] ?? null) !== 'handle') {
        return null;
    }
    $path = $record['path'] ?? null;
    if (!is_string($path)
        || preg_match('/^[A-Za-z][A-Za-z0-9_\/.-]*\.php$/', $path) !== 1
        || str_contains($path, '..')) {
        return null;
    }
    if (!is_sha256_digest($record['digest'] ?? null)) {
        return null;
    }
    $constructor = [];
    if (!is_array($record['constructor'] ?? null) || count($record['constructor']) > 16) {
        return null;
    }
    foreach ($record['constructor'] as $slot) {
        if (!is_array($slot) || count($slot) !== 2
            || !is_string($slot['name'] ?? null)
            || preg_match('/^[a-z][A-Za-z0-9_]*$/', (string) ($slot['name'] ?? '')) !== 1
            || !is_string($slot['type'] ?? null)
            || ($slot['type'] ?? '') === '' || strlen((string) ($slot['type'] ?? '')) > 256) {
            return null;
        }
        $constructor[] = ['name' => $slot['name'], 'type' => $slot['type']];
    }
    if (!is_array($record['parameters'] ?? null)
        || count($record['parameters']) < 1 || count($record['parameters']) > 8) {
        return null;
    }
    $parameters = [];
    foreach ($record['parameters'] as $parameter) {
        if (!is_array($parameter) || count($parameter) < 3 || count($parameter) > 4
            || !is_string($parameter['name'] ?? null)
            || preg_match('/^[a-z][A-Za-z0-9_]*$/', (string) ($parameter['name'] ?? '')) !== 1
            || !is_string($parameter['type'] ?? null)
            || ($parameter['type'] ?? '') === '' || strlen((string) ($parameter['type'] ?? '')) > 256
            || !is_bool($parameter['required'] ?? null)) {
            return null;
        }
        if (array_key_exists('nullable', $parameter) && !is_bool($parameter['nullable'])) {
            return null;
        }
        $parameters[] = $parameter;
    }
    $returnType = $record['returnType'] ?? null;
    if (!is_string($returnType) || $returnType === '' || strlen($returnType) > 256) {
        return null;
    }
    $publicMethods = $record['publicMethods'] ?? null;
    if (!is_array($publicMethods) || count($publicMethods) < 1 || count($publicMethods) > 32) {
        return null;
    }
    foreach ($publicMethods as $method) {
        if (!is_string($method)
            || preg_match('/^[A-Za-z_][A-Za-z0-9_]*$/', $method) !== 1) {
            return null;
        }
    }
    return [
        'id' => $id,
        'fqn' => $fqn,
        'method' => 'handle',
        'path' => $path,
        'digest' => $record['digest'],
        'constructor' => $constructor,
        'parameters' => $parameters,
        'returnType' => $returnType,
        'publicMethods' => $publicMethods,
    ];
}

/**
 * The strict join of every checked and custom record against the
 * observed evidence. `$fileDigest` resolves current source digests.
 * Returns internal finding rows; every refusal happens before any
 * generated write.
 *
 * @param array<int, array<string, mixed>> $records
 * @param callable(string): ?string $fileDigest
 * @return array<int, array<string, string>>
 */
function php_check_operation_bindings(
    array $records,
    array $definitions,
    ?array $evidence,
    array $input,
    string $inputDigest,
    ?callable $fileDigest,
): array {
    $findings = [];
    $add = static function (string $code, string $semanticId, string $detail) use (&$findings): void {
        $findings[] = ['code' => $code, 'semanticId' => $semanticId, 'detail' => $detail];
    };
    // The semantic reconciliation runs first: an unresolvable operation
    // id is the typed finding even when the evidence is absent or
    // stale. The embedded-registry binding equality stays the core
    // join's authority; this keeps the mirror honest when the evidence
    // document is self-authored.
    $unresolved = [];
    foreach ($records as $record) {
        $recordId = (string) $record['id'];
        $definition = $definitions[$recordId] ?? null;
        if (!is_array($definition) || ($definition['kind'] ?? null) !== (string) $record['kind']) {
            $unresolved[$recordId] = true;
            $add(
                'operations.operation-unresolved',
                $recordId,
                "the operation id `$recordId` is not a compiled IR definition of kind `{$record['kind']}`",
            );
        }
    }
    $records = array_values(array_filter(
        $records,
        static fn (array $record): bool => !isset($unresolved[(string) $record['id']]),
    ));
    if ($records === []) {
        return $findings;
    }
    if ($evidence === null) {
        foreach ($records as $record) {
            $add(
                PHP_OPERATIONS_BINDING_MISSING,
                $record['id'],
                'no observed evidence document covers the declared entrypoint',
            );
        }
        return $findings;
    }
    if ($evidence['projectId'] !== $input['projectId']
        || $evidence['irDigest'] !== $input['irDigest']
        || $evidence['operationsInputDigest'] !== $inputDigest) {
        // The evidence names different inputs: every record is stale.
        foreach ($records as $record) {
            $add(
                PHP_OPERATIONS_BINDING_STALE,
                $record['id'],
                'the observed evidence was produced against different input bytes',
            );
        }
        return $findings;
    }
    $byId = [];
    foreach ($evidence['operations'] as $observed) {
        $byId[$observed['id']] = $observed;
    }
    foreach ($records as $record) {
        $id = (string) $record['id'];
        $entry = $record['entry'];
        // The semantic reconciliation the mirror owns: the declared id
        // must be a compiled IR definition of the record's kind. The
        // embedded-registry binding equality stays the core join's
        // authority; this keeps an unresolvable operation a typed
        // finding even when the evidence document is self-authored.
        $definition = $definitions[$id] ?? null;
        if (!is_array($definition) || ($definition['kind'] ?? null) !== (string) $record['kind']) {
            $add(
                'operations.operation-unresolved',
                $id,
                "the operation id `$id` is not a compiled IR definition of kind `{$record['kind']}`",
            );
            continue;
        }
        $matches = array_values(array_filter(
            $byId,
            static fn (array $candidate): bool => strtolower((string) $candidate['fqn']) === strtolower((string) $entry['fqn']),
        ));
        if (count($matches) > 1) {
            $add(PHP_OPERATIONS_BINDING_AMBIGUOUS, $id, 'the evidence carries competing records for the declared FQN');
            continue;
        }
        $observed = $matches[0] ?? null;
        if ($observed === null) {
            $observed = $byId[$id] ?? null;
            if ($observed === null) {
                $add(PHP_OPERATIONS_BINDING_MISSING, $id, 'no observed record carries the declared entrypoint');
                continue;
            }
        }
        $nativePath = (string) $observed['path'];
        if (strtolower($nativePath) !== strtolower((string) $entry['path'])) {
            $add(PHP_OPERATIONS_BINDING_MISMATCH, $id, "the observed path `$nativePath` diverges from the declared `{$entry['path']}`");
            continue;
        }
        // Current bytes: the observed digest must match the live file.
        $live = $fileDigest !== null ? $fileDigest($nativePath) : null;
        if ($live === null) {
            $add(PHP_OPERATIONS_BINDING_STALE, $id, 'the observed source is missing on disk');
            continue;
        }
        if ($live !== $observed['digest']) {
            $add(PHP_OPERATIONS_BINDING_STALE, $id, 'the observed source digest is stale against the live bytes');
            continue;
        }
        $inventory = $evidence['sources'][strtolower($nativePath)] ?? null;
        if ($inventory !== null && $inventory !== $observed['digest']) {
            $add(PHP_OPERATIONS_BINDING_STALE, $id, 'the observed record and the source inventory disagree');
            continue;
        }
        if ((string) $observed['method'] !== (string) $entry['method']) {
            $add(PHP_OPERATIONS_BINDING_MISMATCH, $id, 'the observed entrypoint method diverges');
            continue;
        }
        $publicMethods = $observed['publicMethods'];
        $extra = array_values(array_filter(
            $publicMethods,
            static fn (string $method): bool => !in_array($method, ['handle', '__construct'], true),
        ));
        if ($extra !== []) {
            $add(
                PHP_OPERATIONS_BINDING_MISMATCH,
                $id,
                'the observed class exposes extra public entrypoints: ' . implode(',', $extra),
            );
            continue;
        }
        $parameters = $observed['parameters'];
        if (count($parameters) < 2
            || !str_ends_with((string) $parameters[0]['type'], 'Input')
            || (string) $parameters[1]['type'] !== 'ActorContext'
            || $parameters[0]['required'] !== true
            || $parameters[1]['required'] !== true) {
            $add(
                PHP_OPERATIONS_BINDING_MISMATCH,
                $id,
                'the observed signature is not (input, ActorContext)',
            );
            continue;
        }
        // Constructor slots: the observed class must inject at least
        // one typed dependency (a handler with an empty constructor
        // cannot carry the declared ports) and every slot resolves to a
        // non-empty type identity.
        if (count($observed['constructor']) < 1) {
            $add(
                PHP_OPERATIONS_BINDING_MISMATCH,
                $id,
                'the observed class injects no typed dependency',
            );
            continue;
        }
    }
    // Deterministic order: semantic id, then code.
    usort($findings, static function (array $left, array $right): int {
        return [$left['semanticId'], $left['code']] <=> [$right['semanticId'], $right['code']];
    });
    return $findings;
}

// ----- bundled compiler module: route-policy.php -----


/**
 * The closed routes policy and input validation of the PHP Laravel
 * routes generator (issue #60). Everything here is pure validation over
 * plain data: the bounded routes input document
 * (`lekalo/routes/*.routes.json`, contract
 * `dev.lekalo.php-routes-input@0.4.0`). Nothing reads the filesystem,
 * nothing writes, nothing executes project code.
 *
 * The closed grammar mirrors `contracts/php-routes-input.schema.v0.4.0.json`
 * and the core join (`crates/lekalo-core/src/php_routes/`) exactly:
 * unknown members and unknown enum values refuse. The core join is the
 * acceptance authority; this mirror is the adapter's defensive gate so
 * a request can never reach the emitter half-validated.
 */

const PHP_ROUTES_INPUT_SCHEMA_VERSION = 'lekalo/php-routes-input/v0.4.0';
const PHP_ROUTES_INPUT_IDENTITY = 'dev.lekalo.php-routes-input@0.4.0';
const PHP_ROUTES_MAP_SCHEMA_VERSION = 'lekalo/php-routes-map/v0.4.0';
const PHP_ROUTES_MAP_IDENTITY = 'dev.lekalo.php-routes-map@0.4.0';
const PHP_ROUTES_EVIDENCE_SCHEMA_VERSION = 'lekalo/php-routes-evidence/v0.4.0';
const PHP_ROUTES_EVIDENCE_IDENTITY = 'dev.lekalo.php-routes-evidence@0.4.0';

/** The generated routes root (managed custody). */
const PHP_ROUTES_GENERATED_ROOT = '.lekalo/generated/php-laravel/routes';

/** The observed-route evidence the checked join consumes. */
const PHP_ROUTES_EVIDENCE_PATH = '.lekalo/import/observed/routes-evidence.json';

const PHP_ROUTES_DEFAULT_NAMESPACE_PREFIX = 'Lekalo\\Generated\\Routes';

/** The closed wire identity the staged transport evidence must carry. */
const PHP_ROUTES_TRANSPORT_SCHEMA_VERSION = 'lekalo/transport-http/v0.4.0';
const PHP_ROUTES_TRANSPORT_IDENTITY = 'dev.lekalo.transport-http@0.4.0';

/**
 * Validate one parsed routes input document against its closed shape.
 * Returns the normalized input array, or null when the document is not
 * the accepted contract (a present-but-invalid document is an authoring
 * error, never an all-defaults fallback).
 */
function php_validate_routes_input(mixed $document): ?array
{
    if (!is_array($document)
        || ($document['schemaVersion'] ?? null) !== PHP_ROUTES_INPUT_SCHEMA_VERSION
        || ($document['identity'] ?? null) !== PHP_ROUTES_INPUT_IDENTITY) {
        return null;
    }
    // The closed member set (additionalProperties: false): an unknown
    // member is an authoring error, never a silently ignored hint.
    foreach (array_keys($document) as $member) {
        if (!in_array($member, ['schemaVersion', 'identity', 'projectId', 'irDigest', 'transportDigest', 'typesInputDigest', 'operationsInputDigest', 'policy', 'routes'], true)) {
            return null;
        }
    }
    $projectId = $document['projectId'] ?? null;
    if (!is_string($projectId) || preg_match('/^[a-z][a-z0-9_]*$/', $projectId) !== 1) {
        return null;
    }
    foreach (['irDigest', 'transportDigest', 'typesInputDigest', 'operationsInputDigest'] as $digest) {
        if (!is_sha256_digest($document[$digest] ?? null)) {
            return null;
        }
    }
    $prefix = PHP_ROUTES_DEFAULT_NAMESPACE_PREFIX;
    $middleware = [];
    if (array_key_exists('policy', $document)) {
        $policy = $document['policy'];
        if (!is_array($policy)) {
            return null;
        }
        foreach (array_keys($policy) as $member) {
            if (!in_array($member, ['namespacePrefix', 'middleware'], true)) {
                return null;
            }
        }
        $declared = $policy['namespacePrefix'] ?? null;
        if (!is_string($declared) || preg_match('/^[A-Za-z_][A-Za-z0-9_]*(\\\\[A-Za-z_][A-Za-z0-9_]*){1,5}$/', $declared) !== 1) {
            return null;
        }
        $prefix = $declared;
        if (array_key_exists('middleware', $policy)) {
            if (!is_array($policy['middleware']) || count($policy['middleware']) > 16) {
                return null;
            }
            foreach ($policy['middleware'] as $binding) {
                if (!is_array($binding)) {
                    return null;
                }
                foreach (array_keys($binding) as $member) {
                    if (!in_array($member, ['scheme', 'middleware'], true)) {
                        return null;
                    }
                }
                $scheme = $binding['scheme'] ?? null;
                $spelling = $binding['middleware'] ?? null;
                if (!is_string($scheme) || preg_match('/^[a-z][a-z0-9_]*$/', $scheme) !== 1 || strlen($scheme) > 64) {
                    return null;
                }
                if (!is_string($spelling)
                    || $spelling === ''
                    || strlen($spelling) > 128
                    || preg_match('/^[A-Za-z][A-Za-z0-9:_.-]*$/', $spelling) !== 1) {
                    return null;
                }
                $middleware[] = ['scheme' => $scheme, 'middleware' => $spelling];
            }
            $schemes = array_map(static fn (array $row): string => $row['scheme'], $middleware);
            if (count($schemes) !== count(array_unique($schemes))) {
                return null;
            }
        }
    }
    $routes = $document['routes'] ?? null;
    if (!is_array($routes) || $routes === [] || count($routes) > 2048) {
        return null;
    }
    $records = [];
    foreach ($routes as $route) {
        $record = php_validate_route_record($route);
        if ($record === null) {
            return null;
        }
        $records[] = $record;
    }
    $ids = array_map(static fn (array $record): string => $record['id'], $records);
    $sorted = $ids;
    sort($sorted, SORT_STRING);
    if ($ids !== $sorted || count(array_unique($ids)) !== count($ids)) {
        return null;
    }
    return [
        'projectId' => $projectId,
        'irDigest' => $document['irDigest'],
        'transportDigest' => $document['transportDigest'],
        'typesInputDigest' => $document['typesInputDigest'],
        'operationsInputDigest' => $document['operationsInputDigest'],
        'namespacePrefix' => $prefix,
        'middleware' => $middleware,
        'routes' => $records,
    ];
}

/**
 * One closed route record: the endpoint symbol, the invoked operation,
 * and the per-route custody mode — never a restated wire fact.
 */
function php_validate_route_record(mixed $route): ?array
{
    if (!is_array($route)) {
        return null;
    }
    foreach (array_keys($route) as $member) {
        if (!in_array($member, ['id', 'operation', 'mode', 'entry'], true)) {
            return null;
        }
    }
    $id = $route['id'] ?? null;
    if (!is_string($id) || preg_match('/^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$/', $id) !== 1) {
        return null;
    }
    $operation = $route['operation'] ?? null;
    if (!is_string($operation) || preg_match('/^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$/', $operation) !== 1) {
        return null;
    }
    $mode = $route['mode'] ?? null;
    if ($mode !== 'managed' && $mode !== 'checked') {
        return null;
    }
    $entry = null;
    if (array_key_exists('entry', $route) && $route['entry'] !== null) {
        if ($mode !== 'checked' || !is_array($route['entry'])) {
            return null;
        }
        foreach (array_keys($route['entry']) as $member) {
            if (!in_array($member, ['fqn', 'method'], true)) {
                return null;
            }
        }
        $fqn = $route['entry']['fqn'] ?? null;
        $method = $route['entry']['method'] ?? null;
        if (!is_string($fqn) || preg_match('/^[A-Za-z_][A-Za-z0-9_]*(\\\\[A-Za-z_][A-Za-z0-9_]*){1,7}$/', $fqn) !== 1) {
            return null;
        }
        if (!is_string($method) || preg_match('/^[a-z][A-Za-z0-9_]{0,63}$/', $method) !== 1) {
            return null;
        }
        $entry = ['fqn' => $fqn, 'method' => $method];
    } elseif ($mode === 'checked') {
        return null;
    }
    return ['id' => $id, 'operation' => $operation, 'mode' => $mode, 'entry' => $entry];
}

/**
 * Validate one parsed observed-routes evidence document (the checked
 * route mode's scanner evidence). Returns the normalized document, or
 * null when it is not the accepted contract.
 */
function php_validate_routes_evidence(mixed $document): ?array
{
    if (!is_array($document)
        || ($document['schemaVersion'] ?? null) !== PHP_ROUTES_EVIDENCE_SCHEMA_VERSION
        || ($document['identity'] ?? null) !== PHP_ROUTES_EVIDENCE_IDENTITY) {
        return null;
    }
    foreach (array_keys($document) as $member) {
        if (!in_array($member, ['schemaVersion', 'identity', 'projectId', 'irDigest', 'routesInputDigest', 'producer', 'sources', 'routes'], true)) {
            return null;
        }
    }
    $projectId = $document['projectId'] ?? null;
    if (!is_string($projectId) || preg_match('/^[a-z][a-z0-9_]*$/', $projectId) !== 1) {
        return null;
    }
    foreach (['irDigest', 'routesInputDigest'] as $digest) {
        if (!is_sha256_digest($document[$digest] ?? null)) {
            return null;
        }
    }
    $producer = $document['producer'] ?? null;
    if (!is_array($producer)) {
        return null;
    }
    foreach (array_keys($producer) as $member) {
        if (!in_array($member, ['tool', 'version', 'receiptPath', 'receiptDigest'], true)) {
            return null;
        }
    }
    if (!is_string($producer['tool'] ?? null) || preg_match('/^[a-z][a-z0-9-]{0,31}$/', (string) $producer['tool']) !== 1) {
        return null;
    }
    if (!is_string($producer['version'] ?? null) || preg_match('/^[0-9]+\.[0-9]+\.[0-9]+$/', (string) $producer['version']) !== 1) {
        return null;
    }
    if (array_key_exists('receiptPath', $producer)
        && (!is_string($producer['receiptPath']) || preg_match('/^[.a-z][a-z0-9_\/.-]*\.json$/', (string) $producer['receiptPath']) !== 1)) {
        return null;
    }
    if (array_key_exists('receiptDigest', $producer) && !is_sha256_digest($producer['receiptDigest'])) {
        return null;
    }
    $sources = $document['sources'] ?? null;
    if (!is_array($sources) || $sources === [] || count($sources) > 4096) {
        return null;
    }
    $normalizedSources = [];
    foreach ($sources as $source) {
        if (!is_array($source)) {
            return null;
        }
        foreach (array_keys($source) as $member) {
            if (!in_array($member, ['path', 'digest'], true)) {
                return null;
            }
        }
        $path = $source['path'] ?? null;
        if (!is_string($path) || preg_match('/^[a-z][a-z0-9_\/.-]*\.php$/', (string) $path) !== 1) {
            return null;
        }
        if (!is_sha256_digest($source['digest'] ?? null)) {
            return null;
        }
        $normalizedSources[] = ['path' => $path, 'digest' => $source['digest']];
    }
    $routes = $document['routes'] ?? null;
    if (!is_array($routes) || $routes === [] || count($routes) > 4096) {
        return null;
    }
    $normalizedRoutes = [];
    foreach ($routes as $route) {
        $record = php_validate_observed_route($route);
        if ($record === null) {
            return null;
        }
        $normalizedRoutes[] = $record;
    }
    return [
        'projectId' => $projectId,
        'irDigest' => $document['irDigest'],
        'routesInputDigest' => $document['routesInputDigest'],
        'producer' => $producer,
        'sources' => $normalizedSources,
        'routes' => $normalizedRoutes,
    ];
}

/** One observed route row of the scanner evidence. */
function php_validate_observed_route(mixed $route): ?array
{
    if (!is_array($route)) {
        return null;
    }
    foreach (array_keys($route) as $member) {
        if (!in_array($member, ['method', 'uri', 'name', 'action', 'path', 'digest', 'middleware'], true)) {
            return null;
        }
    }
    $method = $route['method'] ?? null;
    if (!is_string($method) || !in_array($method, ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS'], true)) {
        return null;
    }
    $uri = $route['uri'] ?? null;
    if (!is_string($uri) || preg_match('#^/[A-Za-z0-9_{}/-]*$#', (string) $uri) !== 1) {
        return null;
    }
    $name = $route['name'] ?? null;
    if (!is_string($name) || strlen($name) > 192 || preg_match('/^[a-zA-Z0-9._-]*$/', (string) $name) !== 1) {
        return null;
    }
    $action = $route['action'] ?? null;
    if (!is_string($action) || preg_match('/^[A-Za-z_][A-Za-z0-9_]*(\\\\[A-Za-z_][A-Za-z0-9_]*){1,7}@[a-z][A-Za-z0-9_]{0,63}$/', (string) $action) !== 1) {
        return null;
    }
    $path = $route['path'] ?? null;
    if (!is_string($path) || preg_match('/^[A-Za-z][A-Za-z0-9_\/.-]*\.php$/', (string) $path) !== 1) {
        return null;
    }
    if (!is_sha256_digest($route['digest'] ?? null)) {
        return null;
    }
    $middleware = [];
    if (array_key_exists('middleware', $route)) {
        if (!is_array($route['middleware']) || count($route['middleware']) > 8) {
            return null;
        }
        foreach ($route['middleware'] as $spelling) {
            if (!is_string($spelling) || $spelling === '' || strlen($spelling) > 128 || preg_match('/^[A-Za-z][A-Za-z0-9:_.-]*$/', (string) $spelling) !== 1) {
                return null;
            }
            $middleware[] = $spelling;
        }
        $sorted = $middleware;
        sort($sorted, SORT_STRING);
        if ($middleware !== $sorted) {
            return null;
        }
    }
    return [
        'method' => $method,
        'uri' => $uri,
        'name' => $name,
        'action' => $action,
        'path' => $path,
        'digest' => $route['digest'],
        'middleware' => $middleware,
    ];
}

/** The parsed observed routes evidence, or null when absent/corrupt. */
function php_read_routes_evidence(): ?array
{
    $text = read_view_file(PHP_ROUTES_EVIDENCE_PATH);
    if ($text === null) {
        return null;
    }
    try {
        $document = json_decode($text, true, 512, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return null;
    }
    return php_validate_routes_evidence($document);
}

// ----- bundled compiler module: route-map.php -----


/**
 * The routes mapper (issue #60): the validated routes input joined with
 * the staged evidence — the compiled IR, the canonical transport-http
 * attachment, the OpenAPI projection, and the bound operations input —
 * becomes the deterministic route inventory. Pure: no filesystem, no
 * clock, no environment. A record the authorities cannot join is a
 * typed finding, never a guessed route; the whole run refuses before
 * any emission when any finding exists.
 *
 * Method, path, parameters, body, success and error projections,
 * security, headers, and scenario links come verbatim from the IR and
 * the transport attachment. The error envelope members (category, code,
 * public payload members) come from the OpenAPI projection of the same
 * join — the emitted boundary and the published document are one
 * projection by construction, and the digests are pinned in the
 * custody sidecar.
 */

/** The closed HTTP methods the route layer registers. */
const PHP_ROUTES_METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'];

/**
 * Map the routes inventory. `$context` is `{input, definitions,
 * typesIndex, operationsInput, transport, transportDigest, openapi,
 * irDigest, namespacePrefix}`. Returns `{state, findings, routes}`;
 * every route row carries the decode plan, the envelope table, the
 * emitted-class identities, and the exported links.
 */
function php_map_routes(array $context): array
{
    $findings = [];
    $addFinding = static function (string $code, string $semanticId, string $detail) use (&$findings): void {
        $findings[] = ['code' => $code, 'semanticId' => $semanticId, 'detail' => $detail];
    };
    $input = $context['input'];
    $definitions = $context['definitions'];
    $transport = $context['transport'];
    $openapi = $context['openapi'];

    // The evidence joins: the attachment must bind these exact IR bytes,
    // and the OpenAPI projection must be the render of the same join.
    $transportIrDigest = (string) ($transport['irRef']['digest'] ?? '');
    if ($transportIrDigest !== $context['irDigest']) {
        $addFinding('routes.transport-ir-mismatch', 'php-routes', 'the transport attachment binds different IR bytes than the staged evidence');
    }
    if (!is_array($openapi)) {
        $addFinding('routes.openapi-unbound', 'php-routes', 'the staged OpenAPI projection is missing; the routes boundary and the published document must be one projection');
    } else {
        $provenance = is_array($openapi['x-lekalo-provenance'] ?? null) ? $openapi['x-lekalo-provenance'] : [];
        $openapiIr = (string) ($provenance['irRef']['digest'] ?? '');
        if ($openapiIr !== $context['irDigest']) {
            $addFinding('routes.openapi-ir-mismatch', 'php-routes', 'the OpenAPI projection renders different IR bytes than the staged evidence');
        }
        $openapiTransport = (string) ($provenance['transportRef']['digest'] ?? '');
        if ($openapiTransport !== $context['transportDigest']) {
            $addFinding('routes.openapi-transport-mismatch', 'php-routes', 'the OpenAPI projection renders a different transport attachment than the staged evidence');
        }
    }

    // The middleware scheme index (declared in the input policy).
    $middlewareByScheme = [];
    foreach ($input['middleware'] as $binding) {
        $middlewareByScheme[$binding['scheme']] = $binding['middleware'];
    }

    // The operations input index: the handlers the wrappers invoke.
    $operationsById = [];
    foreach (($context['operationsInput']['operations'] ?? []) as $record) {
        $operationsById[(string) $record['id']] = $record;
    }
    $operationsPrefix = (string) ($context['operationsInput']['namespacePrefix'] ?? '');

    // The OpenAPI error-envelope index: one entry per declared error id
    // with its category, LEK-ERR code, and public payload members.
    $errorEnvelopeIndex = is_array($openapi) ? php_routes_error_envelope_index($openapi) : [];

    $routes = [];
    /** @var array<string, true> $claimedFqns */
    $claimedFqns = [];
    /** @var array<string, true> $claimedPaths */
    $claimedPaths = [];
    foreach ($input['routes'] as $record) {
        $mapped = php_routes_map_record(
            $record,
            $context,
            $definitions,
            $transport,
            $operationsById,
            $operationsPrefix,
            $middlewareByScheme,
            $errorEnvelopeIndex,
            $addFinding,
        );
        if ($mapped === null) {
            continue;
        }
        foreach ($mapped['artifacts'] as $artifact) {
            $fqnKey = strtolower((string) $artifact['fqn']);
            $pathKey = strtolower((string) $artifact['path']);
            if (isset($claimedFqns[$fqnKey]) || isset($claimedPaths[$pathKey])) {
                $addFinding('routes.naming-collision', $mapped['id'], 'the name or path of one emitted route class is claimed twice');
                continue 2;
            }
            $claimedFqns[$fqnKey] = true;
            $claimedPaths[$pathKey] = true;
        }
        $routes[] = $mapped;
    }
    if ($findings !== []) {
        return ['state' => 'unsupported', 'findings' => php_routes_sort_findings($findings)];
    }
    return ['state' => 'mapped', 'findings' => [], 'routes' => $routes];
}

/**
 * The OpenAPI error-envelope index: every `components.schemas` entry
 * carrying an `x-lekalo-symbol` error id and the canonical envelope
 * constants contributes `{category, code, payload[]}`. The projection
 * is the authority; an error the projection never rendered cannot be
 * mapped by the boundary.
 */
function php_routes_error_envelope_index(array $openapi): array
{
    $index = [];
    foreach (($openapi['components']['schemas'] ?? []) as $schema) {
        if (!is_array($schema)) {
            continue;
        }
        $symbol = $schema['x-lekalo-symbol'] ?? null;
        $error = is_array($schema['properties']['error']['properties'] ?? null)
            ? $schema['properties']['error']['properties']
            : null;
        if (!is_string($symbol) || $error === null) {
            continue;
        }
        $category = $error['category']['const'] ?? null;
        $code = $error['code']['const'] ?? null;
        $payload = is_array($error['payload']['properties'] ?? null) ? $error['payload']['properties'] : [];
        if (!is_string($category) || !is_string($code)) {
            continue;
        }
        $index[$symbol] = [
            'category' => $category,
            'code' => $code,
            'payload' => array_keys($payload),
        ];
    }
    return $index;
}

/**
 * Map one route record. Returns null after recording a finding for any
 * unjoinable declaration.
 *
 * @param callable(string, string, string): void $addFinding
 * @return array<string, mixed>|null
 */
function php_routes_map_record(
    array $record,
    array $context,
    array $definitions,
    array $transport,
    array $operationsById,
    string $operationsPrefix,
    array $middlewareByScheme,
    array $errorEnvelopeIndex,
    callable $addFinding,
): ?array {
    $id = (string) $record['id'];
    $operation = (string) $record['operation'];
    $mode = (string) $record['mode'];

    // The endpoint authority: the compiled IR.
    $endpoint = is_array($definitions[$id] ?? null) ? $definitions[$id] : null;
    if ($endpoint === null || ($endpoint['kind'] ?? null) !== 'endpoint') {
        $addFinding('routes.endpoint-unresolved', $id, 'the route id is not a compiled IR endpoint definition');
        return null;
    }
    if ((string) ($endpoint['invokes'] ?? '') !== $operation) {
        $addFinding('routes.invokes-mismatch', $id, 'the endpoint invokes a different operation than the record declares');
        return null;
    }
    $operationDefinition = is_array($definitions[$operation] ?? null) ? $definitions[$operation] : null;
    $operationKind = $operationDefinition === null ? '' : (string) ($operationDefinition['kind'] ?? '');
    if ($operationKind !== 'command' && $operationKind !== 'query') {
        $addFinding('routes.operation-unresolved', $id, 'the invoked operation is not a compiled command or query definition');
        return null;
    }
    // Mode/entry pairing.
    if (($mode === 'checked') !== (is_array($record['entry'] ?? null))) {
        $addFinding('routes.entry-required', $id, $mode === 'checked'
            ? 'a checked record declares its existing controller entrypoint'
            : 'a managed record never declares a foreign entrypoint');
        return null;
    }
    // The wire authority: the transport attachment binding.
    $binding = null;
    foreach ((array) ($transport['endpoints'] ?? []) as $candidate) {
        if (is_array($candidate) && ($candidate['endpoint'] ?? null) === $id) {
            $binding = $candidate;
            break;
        }
    }
    if ($binding === null) {
        $addFinding('routes.transport-unbound', $id, 'the transport attachment binds no wire surface for this endpoint');
        return null;
    }
    $method = (string) ($endpoint['method'] ?? '');
    if (!in_array($method, PHP_ROUTES_METHODS, true)) {
        $addFinding('routes.method-unsupported', $id, 'the endpoint method is outside the closed route-layer vocabulary');
        return null;
    }
    $pathTemplate = (string) ($endpoint['path'] ?? '');
    if ($pathTemplate === '' || $pathTemplate[0] !== '/') {
        $addFinding('routes.path-invalid', $id, 'the endpoint path template is not an absolute route template');
        return null;
    }
    // The handler authority: the bound operations input record.
    $operationRecord = $operationsById[$operation] ?? null;
    if ($operationRecord === null) {
        $addFinding('routes.operation-unbound', $id, 'the invoked operation has no record in the bound operations input');
        return null;
    }
    $entry = php_routes_entry_of($operationRecord, $operationsPrefix, $context['typesIndex'], $context['definitions']);
    if ($entry === null) {
        $addFinding('routes.operation-unbound', $id, 'the operation record carries no joinable typed entrypoint');
        return null;
    }

    // Naming: one thin controller per governed route, named by the
    // endpoint stem; the typed request binding exists for
    // body-carrying managed routes.
    $module = php_types_module_of($id);
    $endpointStem = php_types_stem_of($id, 'plain');
    $controllerClass = $endpointStem . 'Controller';
    $controllerFqn = $context['namespacePrefix'] . '\\' . ucfirst($module) . '\\' . $controllerClass;
    $controllerPath = PHP_ROUTES_GENERATED_ROOT . '/' . php_types_path_of($module, $controllerClass);
    $operationId = (string) ($binding['operationId'] ?? php_routes_camel_of($id));
    $request = null;
    if ($mode === 'managed' && (is_array($binding['body'] ?? null) || (array) ($binding['params'] ?? []) !== [])) {
        // The typed request binding exists for every managed route with
        // a declared decode plan: a body projection or a path-parameter
        // binding (issue #50: the bodyless planning commands) — never a
        // guessed empty input.
        $requestClass = $endpointStem . 'Request';
        $request = [
            'fqn' => $context['namespacePrefix'] . '\\' . ucfirst($module) . '\\' . $requestClass,
            'path' => PHP_ROUTES_GENERATED_ROOT . '/' . php_types_path_of($module, $requestClass),
        ];
    }

    // The middleware attach: only an input-declared scheme mapping
    // attaches middleware; the framework default is never guessed.
    $middleware = [];
    foreach ((array) ($binding['auth']['schemes'] ?? []) as $scheme) {
        $spelling = $middlewareByScheme[(string) $scheme] ?? null;
        if ($spelling !== null && !in_array((string) $spelling, $middleware, true)) {
            $middleware[] = (string) $spelling;
        }
    }
    sort($middleware, SORT_STRING);

    // The decode plan: typed path parameters plus the body projection.
    $decode = php_routes_decode_plan($binding, $operationDefinition, $context['typesIndex'], $context['definitions'], $addFinding, $id);
    if ($decode === null) {
        return null;
    }

    // The declared error table with its envelope constants.
    $errors = [];
    foreach ((array) ($binding['errors'] ?? []) as $declared) {
        $errorId = (string) ($declared['error'] ?? '');
        $envelope = $errorEnvelopeIndex[$errorId] ?? null;
        if ($envelope === null) {
            $addFinding('routes.openapi-error-unbound', $id, "the declared error `{$errorId}` has no envelope constants in the OpenAPI projection");
            return null;
        }
        $errors[] = [
            'error' => $errorId,
            'status' => (int) ($declared['status'] ?? 0),
            'category' => $envelope['category'],
            'code' => $envelope['code'],
            'payload' => $envelope['payload'],
            // The validation-category error is produced by the request
            // binding refusal, never by a handler catch: its `field`
            // payload member lives on the refusal.
            'refusal' => $envelope['category'] === 'validation',
        ];
    }
    usort($errors, static fn (array $left, array $right): int => strcmp($left['error'], $right['error']));
    // The validation binding: a body-carrying managed route declares its
    // validation-category error exactly once — the typed refusal of the
    // request binding maps there, never to an invented id.
    $validationError = null;
    if (is_array($binding['body'] ?? null) && $mode === 'managed') {
        foreach ($errors as $error) {
            if ($error['category'] === 'validation') {
                if ($validationError !== null) {
                    $addFinding('routes.validation-ambiguous', $id, 'the route declares more than one validation-category error');
                    return null;
                }
                $validationError = (string) $error['error'];
            }
        }
        if ($validationError === null) {
            $addFinding('routes.validation-unbound', $id, 'a body-carrying route declares no validation-category error; the decode refusal has no declared mapping');
            return null;
        }
    }
    // The payload members of every declared error must resolve: the
    // `field` member of a validation error comes from the request
    // refusal, every other member is a typed operation input member.
    foreach ($errors as $error) {
        foreach ($error['payload'] as $member) {
            if ($member === 'field' && $error['category'] === 'validation') {
                continue;
            }
            if (php_routes_field_type($member, $operationDefinition, $context['typesIndex'], $context['definitions']) === null) {
                $addFinding('routes.payload-unresolved', $id, "the payload member `{$member}` of `{$error['error']}` does not resolve to a typed input member");
                return null;
            }
        }
    }
    $defaultsBinding = is_array($binding['errorDefaults'] ?? null) ? $binding['errorDefaults'] : [];
    $errorDefaults = [];
    foreach (['validation', 'auth', 'conflict', 'not-found', 'domain', 'infrastructure'] as $category) {
        $errorDefaults[$category] = (int) ($defaultsBinding[$category] ?? 0);
    }

    $success = is_array($binding['success'] ?? null) ? $binding['success'] : [];
    $auth = is_array($binding['auth'] ?? null) ? [
        'actor' => (string) $binding['auth']['actor'],
        'schemes' => array_map('strval', (array) ($binding['auth']['schemes'] ?? [])),
        'policyRef' => isset($binding['auth']['policyRef']) ? (string) $binding['auth']['policyRef'] : null,
    ] : null;

    $artifacts = [];
    if ($mode === 'managed') {
        $artifacts[] = ['path' => $controllerPath, 'fqn' => $controllerFqn];
        if ($request !== null) {
            $artifacts[] = ['path' => $request['path'], 'fqn' => $request['fqn']];
        }
    }

    return [
        'id' => $id,
        'operation' => $operation,
        'operationKind' => $operationKind,
        'operationId' => $operationId,
        'mode' => $mode,
        'method' => $method,
        'pathTemplate' => $pathTemplate,
        'name' => $operationId,
        'controller' => ['fqn' => $controllerFqn, 'path' => $controllerPath, 'action' => $operationId],
        'request' => $request,
        'entry' => $entry,
        'middleware' => $middleware,
        'success' => [
            'status' => (int) ($success['status'] ?? 0),
            'bodyMode' => isset($success['body']['mode']) ? (string) $success['body']['mode'] : null,
        ],
        'errors' => $errors,
        'errorDefaults' => $errorDefaults,
        'validationError' => $validationError,
        'auth' => $auth,
        'idempotency' => is_array($binding['idempotency'] ?? null) ? [
            'header' => (string) $binding['idempotency']['header'],
            'required' => (bool) $binding['idempotency']['required'],
        ] : null,
        'correlation' => is_array($binding['correlation'] ?? null) ? [
            'headers' => array_map('strval', (array) ($binding['correlation']['headers'] ?? [])),
        ] : null,
        'scenarios' => array_map('strval', (array) ($binding['scenarios'] ?? [])),
        'decode' => $decode,
        'links' => array_filter([
            'endpoint' => $id,
            'operation' => $operation,
            'controller' => $mode === 'managed' ? $controllerFqn : null,
            'request' => $request['fqn'] ?? null,
            'scenarios' => array_map('strval', (array) ($binding['scenarios'] ?? [])),
            'openapiPointer' => '/paths/' . php_routes_escape_pointer($pathTemplate) . '/' . strtolower($method),
        ], static fn (mixed $value): bool => $value !== null),
        'artifacts' => $artifacts,
    ];
}

/**
 * The invoked operation entrypoint: managed and scaffold operations
 * derive the generated handler spelling; checked and custom operations
 * carry the declared existing entry. The typed input identity joins the
 * #58 inventory (commands) or the operations family namespace
 * (queries).
 *
 * @return array{fqn: string, method: string, inputFqn: string}|null
 */
function php_routes_entry_of(array $operationRecord, string $operationsPrefix, array $typesIndex, array $definitions): ?array
{
    $id = (string) $operationRecord['id'];
    $module = php_types_module_of($id);
    $stem = php_types_stem_of($id, 'plain');
    $inputFqn = php_routes_entry_input_fqn($operationRecord, $operationsPrefix, $typesIndex);
    if ($inputFqn === null) {
        return null;
    }
    $mode = (string) $operationRecord['mode'];
    if (in_array($mode, ['managed', 'scaffold-once'], true)) {
        return [
            'fqn' => $operationsPrefix . '\\' . ucfirst($module) . '\\' . $stem . 'Handler',
            'method' => 'handle',
            'inputFqn' => $inputFqn,
        ];
    }
    $entry = is_array($operationRecord['entry'] ?? null) ? $operationRecord['entry'] : null;
    if ($entry === null) {
        return null;
    }
    return ['fqn' => (string) $entry['fqn'], 'method' => (string) $entry['method'], 'inputFqn' => $inputFqn];
}

/**
 * The typed input FQN of one operation record (sidecar-neutral helper
 * of `php_routes_entry_of`).
 */
function php_routes_entry_input_fqn(array $operationRecord, string $operationsPrefix, array $typesIndex): ?string
{
    $id = (string) $operationRecord['id'];
    $module = php_types_module_of($id);
    $stem = php_types_stem_of($id, 'plain');
    if ((string) $operationRecord['kind'] === 'command') {
        $inputFqn = is_array($typesIndex[$id] ?? null) ? (string) ($typesIndex[$id]['fqn'] ?? '') : '';
        return $inputFqn === '' ? null : $inputFqn;
    }
    return $operationsPrefix . '\\' . ucfirst($module) . '\\' . $stem . 'Input';
}

/**
 * The decode plan of one binding: typed path parameters plus the body
 * mode. The parameter and body field types resolve through the mapped
 * type inventory — an unresolvable field is a finding, never a loose
 * array.
 *
 * @param callable(string, string, string): void $addFinding
 * @return array<string, mixed>|null
 */
function php_routes_decode_plan(array $binding, array $operationDefinition, array $typesIndex, array $definitions, callable $addFinding, string $routeId): ?array
{
    $params = [];
    foreach ((array) ($binding['params'] ?? []) as $param) {
        $type = php_routes_field_type($param['field'] ?? null, $operationDefinition, $typesIndex, $definitions);
        if ($type === null) {
            $addFinding('routes.field-unresolved', $routeId, 'a declared parameter field does not resolve to a typed operation input member');
            return null;
        }
        $params[] = [
            'name' => (string) $param['name'],
            'in' => (string) $param['in'],
            'field' => (string) $param['field'],
            'required' => (bool) ($param['required'] ?? false),
            'type' => $type,
        ];
    }
    $body = null;
    if (is_array($binding['body'] ?? null)) {
        $fields = [];
        foreach ((array) ($binding['body']['fields'] ?? []) as $field) {
            $type = php_routes_field_type($field['field'] ?? null, $operationDefinition, $typesIndex, $definitions);
            if ($type === null) {
                $addFinding('routes.field-unresolved', $routeId, 'a declared body field does not resolve to a typed operation input member');
                return null;
            }
            $fields[] = [
                'name' => (string) $field['name'],
                'field' => (string) $field['field'],
                'required' => (bool) ($field['required'] ?? false),
                'type' => $type,
            ];
        }
        $body = ['mode' => (string) $binding['body']['mode'], 'fields' => $fields];
    }
    return ['params' => $params, 'body' => $body];
}

/**
 * The mapped type identity of one declared field reference
 * (`input.<name>` for command inputs): the FQN rides the #58 mapped
 * inventory, the wire kind rides the compiled IR definition.
 *
 * @return array{ref: string, fqn: string, kind: string, base: string}|null
 */
function php_routes_field_type(mixed $field, array $operationDefinition, array $typesIndex, array $definitions): ?array
{
    if (!is_string($field) || $field === '') {
        return null;
    }
    $member = str_starts_with($field, 'input.') ? substr($field, 6) : $field;
    foreach ((array) ($operationDefinition['input'] ?? []) as $inputField) {
        if (!is_array($inputField) || ($inputField['name'] ?? null) !== $member) {
            continue;
        }
        $ref = $inputField['type']['ref'] ?? null;
        if (!is_string($ref)) {
            return null;
        }
        $entry = is_array($typesIndex[$ref] ?? null) ? $typesIndex[$ref] : null;
        $definition = is_array($definitions[$ref] ?? null) ? $definitions[$ref] : null;
        if ($entry === null || $definition === null) {
            return null;
        }
        return [
            'ref' => $ref,
            'fqn' => (string) $entry['fqn'],
            'kind' => (string) ($definition['kind'] ?? ''),
            'base' => (string) ($definition['base'] ?? ''),
        ];
    }
    return null;
}

/** The deterministic camel spelling of the default operation id. */
function php_routes_camel_of(string $semanticId): string
{
    $parts = preg_split('/[._]/', $semanticId) ?: [];
    $camel = array_shift($parts) ?? '';
    foreach ($parts as $part) {
        $camel .= ucfirst($part);
    }
    return $camel;
}

/** The RFC 6901 pointer escape of one path-template token. */
function php_routes_escape_pointer(string $template): string
{
    return str_replace(['~', '/'], ['~0', '~1'], $template);
}

/** Findings sort deterministically by semantic id, then code. */
function php_routes_sort_findings(array $findings): array
{
    usort($findings, static function (array $left, array $right): int {
        return [$left['semanticId'], $left['code']] <=> [$right['semanticId'], $right['code']];
    });
    return $findings;
}

/**
 * The checked-route join (issue #60): the declared surface of every
 * checked record joins against the observed routes evidence. Absent
 * evidence is a finding for every declared id; a stale controller
 * digest, a foreign method/uri/name/action, or a partial record is a
 * typed finding — never a pass, never a silent rewrite. No write ever
 * results.
 *
 * @param array $records the checked route records of the validated input
 * @param array $mappedRoutes the mapped route rows by endpoint id
 * @param array $definitions the IR evidence definitions by id
 * @param array|null $evidence the parsed observed routes evidence
 * @param array $input the validated routes input
 * @param string $inputDigest the digest of the exact input bytes
 * @param callable(string): ?string $digestAt the source digest probe
 * @return list<array{code: string, semanticId: string, detail: string}>
 */
function php_routes_check_bindings(
    array $records,
    array $mappedRoutes,
    array $definitions,
    ?array $evidence,
    array $input,
    string $inputDigest,
    callable $digestAt,
): array {
    $findings = [];
    if ($evidence === null) {
        foreach ($records as $record) {
            $findings[] = [
                'code' => 'routes.binding-missing',
                'semanticId' => (string) $record['id'],
                'detail' => 'the observed routes evidence is absent; a checked route never passes without scanner evidence',
            ];
        }
        return $findings;
    }
    // The evidence binds these exact IR and input bytes; a stale pin
    // makes every record unknown.
    if ($evidence['irDigest'] !== $input['irDigest'] || $evidence['routesInputDigest'] !== $inputDigest) {
        foreach ($records as $record) {
            $findings[] = [
                'code' => 'routes.binding-stale',
                'semanticId' => (string) $record['id'],
                'detail' => 'the observed routes evidence pins different IR or input bytes',
            ];
        }
        return $findings;
    }
    /** @var array<string, list<array>> $byRoute the observed rows keyed by (method, uri) */
    $byMethodUri = [];
    foreach ($evidence['routes'] as $row) {
        $byMethodUri[$row['method'] . ' ' . $row['uri']][] = $row;
    }
    foreach ($records as $record) {
        $id = (string) $record['id'];
        $mapped = null;
        foreach ($mappedRoutes as $route) {
            if ($route['id'] === $id) {
                $mapped = $route;
                break;
            }
        }
        if ($mapped === null) {
            $findings[] = ['code' => 'routes.binding-missing', 'semanticId' => $id, 'detail' => 'the checked record has no mapped route row'];
            continue;
        }
        $entry = $record['entry'];
        $candidates = $byMethodUri[$mapped['method'] . ' ' . $mapped['pathTemplate']] ?? [];
        if ($candidates === []) {
            $findings[] = [
                'code' => 'routes.binding-missing',
                'semanticId' => $id,
                'detail' => 'no observed route carries the declared method and uri',
            ];
            continue;
        }
        $matches = [];
        foreach ($candidates as $row) {
            $action = $entry['fqn'] . '@' . $entry['method'];
            if ($row['action'] === $action) {
                $matches[] = $row;
            }
        }
        if (count($matches) === 0) {
            $findings[] = [
                'code' => 'routes.binding-mismatch',
                'semanticId' => $id,
                'detail' => 'the observed routes carry the surface but not the declared entrypoint',
            ];
            continue;
        }
        if (count($matches) > 1) {
            $findings[] = [
                'code' => 'routes.binding-ambiguous',
                'semanticId' => $id,
                'detail' => 'more than one observed route carries the declared surface and action',
            ];
            continue;
        }
        $row = $matches[0];
        // Current bytes: the observed digest must match the live file
        // (the probe keeps the evidence's case-preserving native path).
        $digest = $digestAt($row['path']);
        if ($digest === null) {
            $findings[] = [
                'code' => 'routes.binding-missing',
                'semanticId' => $id,
                'detail' => 'the observed controller source is unreadable',
            ];
            continue;
        }
        if ($digest !== $row['digest']) {
            $findings[] = [
                'code' => 'routes.binding-stale',
                'semanticId' => $id,
                'detail' => 'the observed controller bytes diverge from the evidence digest',
            ];
            continue;
        }
        // The declared middleware attach must match the observed one.
        $observed = $row['middleware'];
        sort($observed, SORT_STRING);
        if ($observed !== $mapped['middleware']) {
            $findings[] = [
                'code' => 'routes.binding-mismatch',
                'semanticId' => $id,
                'detail' => 'the observed middleware differs from the declared scheme mapping',
            ];
        }
    }
    return $findings;
}

// ----- bundled compiler module: route-emit.php -----


/**
 * The routes emitter (issue #60): the mapped routes inventory becomes
 * deterministic PHP bytes — the route registrations, thin
 * decode/delegate/encode controllers, typed request bindings, the
 * shared envelope and validation primitives, the explicit error-to-HTTP
 * map, the digest-bound OpenAPI projection, the classmap, and the
 * custody sidecar. Pure: no filesystem, no clock, no environment.
 * Identical inputs and pins emit identical bytes across roots.
 *
 * The controllers are thin wrappers and nothing else: they decode the
 * transport surface into the #58 typed input, bind the actor, invoke
 * the one operation entrypoint, encode the success projection, and map
 * exactly the declared typed errors through the declared table. There
 * is no catch-all: any throwable outside the declared table propagates
 * to the application's own exception handling, unmodified.
 */

/** The file header of one emitted routes artifact. */
function php_routes_file_header(array $context, string $namespace, string $semanticId): string
{
    $lines = [
        '<?php',
        '',
        'declare(strict_types=1);',
        '',
        '// Generated by ' . ADAPTER_ID . '@' . ADAPTER_VERSION . ' (routes generator, issue #60).',
        '// From dev.lekalo.ir@0.2.16 input ' . $context['irDigest'] . '.',
        '// Semantic id: ' . $semanticId . '.',
        '// Do not edit: regenerate with `lekalo generate`.',
        '',
        'namespace ' . $namespace . ';',
    ];
    return implode("\n", $lines);
}

/**
 * Emit every artifact of one mapped routes inventory. `$input` is
 * `{projectId, mapped, context, openapiBytes, root}`. Returns `files`;
 * every file row carries path/text/digest/role/fqn/lifecycle.
 */
function php_emit_routes(array $input): array
{
    $context = $input['context'];
    $mapped = $input['mapped'];
    $root = $input['root'];
    $namespace = $context['namespacePrefix'];
    $files = [];

    // The shared boundary primitives: the canonical envelope encoder,
    // the typed validation refusal, and the declared error-to-HTTP map.
    $shared = [
        ['http-envelope.php', 'HttpEnvelope', 'envelope', php_routes_http_envelope_text($context, $namespace, (string) $context['operationsNamespacePrefix'])],
        ['request-validation-error.php', 'RequestValidationError', 'validation-error', php_routes_validation_error_text($context, $namespace)],
        ['error-http-map.php', 'ErrorHttpMap', 'error-map', php_routes_error_http_map_text($context, $namespace, $mapped['routes'])],
    ];
    foreach ($shared as [$fileName, $class, $role, $text]) {
        $files[] = [
            'path' => $root . '/' . $fileName,
            'text' => $text,
            'digest' => 'sha256:' . hash('sha256', $text),
            'role' => $role,
            'fqn' => $namespace . '\\' . $class,
            'lifecycle' => 'generated',
        ];
    }

    // The per-route wrappers: request binding plus thin controller.
    foreach ($mapped['routes'] as $route) {
        if ($route['mode'] !== 'managed') {
            continue;
        }
        if ($route['request'] !== null) {
            $text = php_routes_request_text($context, $namespace, $route);
            $files[] = [
                'path' => $route['request']['path'],
                'text' => $text,
                'digest' => 'sha256:' . hash('sha256', $text),
                'role' => 'request',
                'fqn' => $route['request']['fqn'],
                'lifecycle' => 'generated',
            ];
        }
        $text = php_routes_controller_text($context, $namespace, $route);
        $files[] = [
            'path' => $route['controller']['path'],
            'text' => $text,
            'digest' => 'sha256:' . hash('sha256', $text),
            'role' => 'controller',
            'fqn' => $route['controller']['fqn'],
            'lifecycle' => 'generated',
        ];
    }

    // The route registrations: one block per managed route, verbatim
    // from the joined authorities. No discovery, no glob, no magic.
    $routesText = php_routes_registrations_text($context, $mapped['routes']);
    $files[] = [
        'path' => $root . '/routes.php',
        'text' => $routesText,
        'digest' => 'sha256:' . hash('sha256', $routesText),
        'role' => 'routes-file',
        'lifecycle' => 'generated',
    ];

    // The OpenAPI projection: the staged evidence bytes, verbatim and
    // digest-bound — code and document are projections of one join.
    $openapiBytes = (string) $input['openapiBytes'];
    $files[] = [
        'path' => $root . '/openapi.json',
        'text' => $openapiBytes,
        'digest' => 'sha256:' . hash('sha256', $openapiBytes),
        'role' => 'openapi',
        'lifecycle' => 'generated',
    ];

    $classmapText = php_routes_classmap_text($files, $root);
    $files[] = [
        'path' => $root . '/classmap.php',
        'text' => $classmapText,
        'digest' => 'sha256:' . hash('sha256', $classmapText),
        'role' => 'classmap',
        'lifecycle' => 'generated',
    ];

    $sidecarText = php_routes_sidecar_text($input['projectId'], $mapped, $files, $context);
    $files[] = [
        'path' => $root . '/routes.map.json',
        'text' => $sidecarText,
        'digest' => 'sha256:' . hash('sha256', $sidecarText),
        'role' => 'map',
        'lifecycle' => 'generated',
    ];

    usort($files, static fn (array $left, array $right): int => strcmp((string) $left['path'], (string) $right['path']));
    return ['files' => $files];
}

/** The canonical envelope encoder text. */
function php_routes_http_envelope_text(array $context, string $namespace, string $operationsPrefix): string
{
    $actorFqn = '\\' . $operationsPrefix . '\\ActorContext';
    $lines = [
        php_routes_file_header($context, $namespace, 'php-routes/http-envelope'),
        '',
        '/**',
        ' * The canonical-v1 wire envelope of the route boundary (issue #60).',
        ' * The error shape is the declared identity quadruple with the public',
        ' * payload members only; the actor binding carries the declared scope',
        ' * dimensions of the authenticated principal.',
        ' */',
        'final class HttpEnvelope',
        '{',
        '    /**',
        '     * The request attribute the authentication middleware binds the',
        '     * ActorContext under. An auth-bound route without it is an',
        '     * infrastructure misconfiguration, never an anonymous fallback.',
        '     */',
        "    public const ACTOR_ATTRIBUTE = 'lekalo.actor';",
        '',
        '    /**',
        '     * One canonical error envelope: the declared identity members',
        '     * plus the public payload. An empty payload stays an empty JSON',
        '     * object, never an array.',
        '     *',
        '     * @param array<string, mixed> $payload',
        '     * @return array<string, mixed>',
        '     */',
        '    public static function error(string $id, string $code, string $category, array $payload): array',
        '    {',
        "        return ['ok' => false, 'error' => ['id' => \$id, 'code' => \$code, 'category' => \$category, 'payload' => \$payload === [] ? new \\stdClass() : \$payload]];",
        '    }',
        '',
        '    /**',
        '     * The actor binding of one request: the middleware-bound context',
        '     * on an auth-bound route, the declared anonymous context on a',
        '     * public one.',
        '     */',
        '    public static function actor(\\Illuminate\\Http\\Request $request, bool $authRequired): ' . $actorFqn,
        '    {',
        '        if (!$authRequired) {',
        '            return new ' . $actorFqn . "('anonymous');",
        '        }',
        '        $actor = $request->attributes->get(self::ACTOR_ATTRIBUTE);',
        '        if (!$actor instanceof ' . $actorFqn . ') {',
        "            throw new \\RuntimeException('the auth middleware never bound the actor context');",
        '        }',
        '        return $actor;',
        '    }',
        '}',
    ];
    return implode("\n", $lines) . "\n";
}

/** The typed validation-refusal text. */
function php_routes_validation_error_text(array $context, string $namespace): string
{
    $lines = [
        php_routes_file_header($context, $namespace, 'php-routes/request-validation-error'),
        '',
        '/**',
        ' * The typed refusal of one request binding: the decoded surface',
        ' * violates the declared decode plan. The boundary maps it to the',
        ' * declared validation error and status — never to a guessed id.',
        ' */',
        'final class RequestValidationError extends \\RuntimeException',
        '{',
        '    /**',
        '     * @param list<string> $fields the declared member names that',
        '     *     refused (the failing body members or required headers)',
        '     */',
        '    public function __construct(',
        '        public readonly array $fields,',
        '    ) {',
        "        parent::__construct('request validation: ' . implode(',', \$fields));",
        '    }',
        '',
        '    /** The first refused member: the declared `field` payload. */',
        '    public function field(): string',
        '    {',
        '        return $this->fields[0] ?? \'\';',
        '    }',
        '}',
    ];
    return implode("\n", $lines) . "\n";
}

/** The snake-const spelling of one semantic id for const-table names. */
function php_routes_const_of(string $semanticId): string
{
    return 'TABLE_' . strtoupper((string) preg_replace('/[^A-Za-z0-9]/', '_', $semanticId));
}

/**
 * The explicit error-to-HTTP map text: one const table per governed
 * route with the declared status, category, and code per error id,
 * plus the declared category defaults and the validation error id.
 */
function php_routes_error_http_map_text(array $context, string $namespace, array $routes): string
{
    $lines = [
        php_routes_file_header($context, $namespace, 'php-routes/error-http-map'),
        '',
        '/**',
        ' * The explicit error-to-HTTP projection of the governed routes',
        ' * (issue #60): the declared per-error table and the category',
        ' * defaults, exactly as the transport attachment and the OpenAPI',
        ' * projection declare them. An undeclared error id is never mapped:',
        ' * the boundary throws, and the application exception handling owns',
        ' * the failure.',
        ' */',
        'final class ErrorHttpMap',
        '{',
    ];
    foreach ($routes as $route) {
        $const = php_routes_const_of((string) $route['id']);
        $lines[] = '    private const ' . $const . ' = [';
        foreach ($route['errors'] as $error) {
            $lines[] = '        ' . php_types_string_literal((string) $error['error']) . ' => ['
                . "'status' => " . (int) $error['status'] . ', '
                . "'category' => " . php_types_string_literal((string) $error['category']) . ', '
                . "'code' => " . php_types_string_literal((string) $error['code']) . '],';
        }
        $lines[] = '    ];';
    }
    $lines[] = '';
    $lines[] = '    /** @var array<string, array<string, int>> */';
    $lines[] = '    private const ROUTE_DEFAULTS = [';
    foreach ($routes as $route) {
        $lines[] = '        ' . php_types_string_literal((string) $route['id']) . ' => [';
        foreach ($route['errorDefaults'] as $category => $status) {
            $lines[] = '            ' . php_types_string_literal((string) $category) . ' => ' . (int) $status . ',';
        }
        $lines[] = '        ],';
    }
    $lines[] = '    ];';
    $lines[] = '';
    $lines[] = '    /** @var array<string, string> the validation-category error of every body-carrying route. */';
    $lines[] = '    private const VALIDATION_ERRORS = [';
    foreach ($routes as $route) {
        if (isset($route['validationError'])) {
            $lines[] = '        ' . php_types_string_literal((string) $route['id']) . ' => '
                . php_types_string_literal((string) $route['validationError']) . ',';
        }
    }
    $lines[] = '    ];';
    $lines[] = '';
    $lines[] = '    /** @var array<string, array<string, array{status: int, category: string, code: string}>> */';
    $lines[] = '    private const ROUTE_ERRORS = [';
    foreach ($routes as $route) {
        $lines[] = '        ' . php_types_string_literal((string) $route['id']) . ' => self::' . php_routes_const_of((string) $route['id']) . ',';
    }
    $lines[] = '    ];';
    $lines[] = '';
    $lines[] = '    /**';
    $lines[] = '     * One mapped domain failure: the declared envelope of one declared';
    $lines[] = '     * error id, with the wire-encoded public payload members.';
    $lines[] = '     *';
    $lines[] = '     * @param array<string, mixed> $payload';
    $lines[] = '     * @return array{ok: bool, error: array<string, mixed>}|null';
    $lines[] = '     */';
    $lines[] = '    public static function domain(string $endpoint, string $errorId, array $payload): ?array';
    $lines[] = '    {';
    $lines[] = '        $declared = self::ROUTE_ERRORS[$endpoint][$errorId] ?? null;';
    $lines[] = '        if ($declared === null) {';
    $lines[] = '            return null;';
    $lines[] = '        }';
    $lines[] = '        return HttpEnvelope::error($errorId, $declared[\'code\'], $declared[\'category\'], $payload);';
    $lines[] = '    }';
    $lines[] = '';
    $lines[] = '    /** The declared HTTP status of one declared error id. */';
    $lines[] = '    public static function status(string $endpoint, string $errorId): int';
    $lines[] = '    {';
    $lines[] = '        $declared = self::ROUTE_ERRORS[$endpoint][$errorId] ?? null;';
    $lines[] = '        if ($declared === null) {';
    $lines[] = '            throw new LogicException(\'undeclared error id: \' . $errorId);';
    $lines[] = '        }';
    $lines[] = '        return $declared[\'status\'];';
    $lines[] = '    }';
    $lines[] = '';
    $lines[] = '    /**';
    $lines[] = '     * One mapped validation refusal: the declared validation error of';
    $lines[] = '     * the route and the declared validation-category status.';
    $lines[] = '     *';
    $lines[] = '     * @param list<string> $fields';
    $lines[] = '     * @return array{ok: bool, error: array<string, mixed>}|null';
    $lines[] = '     */';
    $lines[] = '    public static function validation(string $endpoint, array $fields): ?array';
    $lines[] = '    {';
    $lines[] = '        $errorId = self::VALIDATION_ERRORS[$endpoint] ?? null;';
    $lines[] = '        if ($errorId === null) {';
    $lines[] = '            return null;';
    $lines[] = '        }';
    $lines[] = '        $mapped = self::domain($endpoint, $errorId, [\'field\' => $fields === [] ? \'\' : (string) $fields[0]]);';
    $lines[] = '        return $mapped;';
    $lines[] = '    }';
    $lines[] = '';
    $lines[] = '    /** The declared status of one mapped validation refusal. */';
    $lines[] = '    public static function validationStatus(string $endpoint): int';
    $lines[] = '    {';
    $lines[] = '        $defaults = self::ROUTE_DEFAULTS[$endpoint] ?? [];';
    $lines[] = '        return (int) ($defaults[\'validation\'] ?? 500);';
    $lines[] = '    }';
    $lines[] = '}';
    return implode("\n", $lines) . "\n";
}

/**
 * The wire-encoding expression of one typed value: string-backed enums
 * spell their declared case, string-backed scalars spell their exact
 * string, everything else spells its primitive value.
 *
 * @param array{ref: string, fqn: string, kind: string, base: string} $type
 */
function php_routes_wire_expr(array $type, string $expr): string
{
    if ($type['kind'] === 'enum') {
        return $expr . '->toWire()';
    }
    if ($type['kind'] === 'scalar' && !in_array($type['base'], ['number', 'boolean'], true)) {
        return $expr . '->toString()';
    }
    return $expr . '->value()';
}

/** The typed request-binding text of one managed body-carrying route. */
function php_routes_request_text(array $context, string $namespace, array $route): string
{
    $requestFqn = (string) $route['request']['fqn'];
    $requestNamespace = substr($requestFqn, 0, (int) strrpos($requestFqn, '\\'));
    $requestClass = substr($requestFqn, (int) strlen($requestNamespace) + 1);
    $inputFqn = (string) $route['entry']['inputFqn'];
    $decode = $route['decode'];
    $lines = [
        php_routes_file_header($context, $requestNamespace, (string) $route['id']),
        '',
        '/**',
        ' * The typed request binding of ' . $route['id'] . ': the declared',
        ' * decode plan (path parameters, body projection) becomes the #58',
        ' * typed input. Any refusal is the typed validation error the',
        ' * controller maps through the declared table.',
        ' */',
        'final class ' . $requestClass,
        '{',
        '    /**',
        '     * Decode one HTTP request into the typed operation input.',
        '     */',
        '    public static function fromWire(\\Illuminate\\Http\\Request $request): \\' . $inputFqn,
        '    {',
    ];
    $hasParams = $decode['params'] !== [];
    $hasExplicitBody = $decode['body'] !== null && $decode['body']['mode'] === 'explicit';
    $isWholeBody = $decode['body'] !== null && $decode['body']['mode'] === 'whole-input';
    // The declared required headers are part of the decode plan: the
    // projection documents them, so the binding enforces them.
    $requiredHeaders = [];
    if (($route['idempotency']['required'] ?? false) === true) {
        $requiredHeaders[] = (string) $route['idempotency']['header'];
    }
    foreach ($requiredHeaders as $header) {
        $lines[] = '        if ($request->header(' . php_types_string_literal($header) . ') === null) {';
        $lines[] = '            throw new \\Lekalo\\Generated\\Routes\\RequestValidationError([' . php_types_string_literal($header) . ']);';
        $lines[] = '        }';
    }
    if ($isWholeBody) {
        $lines[] = '        $wire = $request->json()->all();';
        $lines[] = '        if (!is_array($wire)) {';
        $lines[] = "            throw new \\Lekalo\\Generated\\Routes\\RequestValidationError(['body']);";
        $lines[] = '        }';
    } else {
        $lines[] = '        $wire = [];';
        if ($hasParams) {
            $lines[] = '        $route = $request->route();';
        }
        foreach ($decode['params'] as $param) {
            $member = php_routes_input_member_of((string) $param['field']);
            $lines[] = '        $wire[' . php_types_string_literal($member) . '] = $route->parameter(' . php_types_string_literal((string) $param['name']) . ');';
        }
        if ($hasExplicitBody) {
            $lines[] = '        $body = $request->json()->all();';
            $lines[] = '        if (!is_array($body)) {';
            $lines[] = "            throw new \\Lekalo\\Generated\\Routes\\RequestValidationError(['body']);";
            $lines[] = '        }';
            foreach ($decode['body']['fields'] as $field) {
                $member = php_routes_input_member_of((string) $field['field']);
                if ((bool) $field['required']) {
                    $lines[] = '        if (!array_key_exists(' . php_types_string_literal((string) $field['name']) . ', $body)) {';
                    $lines[] = '            throw new \\Lekalo\\Generated\\Routes\\RequestValidationError([' . php_types_string_literal((string) $field['name']) . ']);';
                    $lines[] = '        }';
                    $lines[] = '        $wire[' . php_types_string_literal($member) . '] = $body[' . php_types_string_literal((string) $field['name']) . '];';
                } else {
                    $lines[] = '        if (array_key_exists(' . php_types_string_literal((string) $field['name']) . ', $body)) {';
                    $lines[] = '            $wire[' . php_types_string_literal($member) . '] = $body[' . php_types_string_literal((string) $field['name']) . '];';
                    $lines[] = '        }';
                }
            }
        }
    }
    $lines[] = '        try {';
    $lines[] = '            return \\' . $inputFqn . 'Codec::decode($wire);';
    $lines[] = '        } catch (\\InvalidArgumentException $failure) {';
    $lines[] = '            throw new \\Lekalo\\Generated\\Routes\\RequestValidationError([], $failure);';
    $lines[] = '        }';
    $lines[] = '    }';
    $lines[] = '}';
    return implode("\n", $lines) . "\n";
}

/** The typed input member of one declared field reference. */
function php_routes_input_member_of(string $field): string
{
    return str_starts_with($field, 'input.') ? substr($field, 6) : $field;
}

/** The thin controller text of one managed route. */
function php_routes_controller_text(array $context, string $namespace, array $route): string
{
    $controllerFqn = (string) $route['controller']['fqn'];
    $controllerNamespace = substr($controllerFqn, 0, (int) strrpos($controllerFqn, '\\'));
    $controllerClass = substr($controllerFqn, (int) strlen($controllerNamespace) + 1);
    $action = (string) $route['controller']['action'];
    $entryFqn = (string) $route['entry']['fqn'];
    $inputFqn = (string) $route['entry']['inputFqn'];
    $routeId = (string) $route['id'];
    $authRequired = $route['auth'] !== null;
    $errorBase = (string) $context['operationsNamespacePrefix'];
    $isCommand = $route['operationKind'] === 'command';

    $lines = [
        php_routes_file_header($context, $controllerNamespace, $routeId),
        '',
        'use Symfony\\Component\\HttpFoundation\\Response;',
        'use Illuminate\\Http\\Request;',
        '',
        '/**',
        ' * The thin HTTP wrapper of ' . $routeId . ' (issue #60): decode the',
        ' * declared transport surface, bind the actor, invoke the one',
        ' * operation entrypoint, and encode the declared projections. There',
        ' * is no business logic and no catch-all: only the declared typed',
        ' * errors map; everything else propagates unmodified.',
        ' */',
        'final readonly class ' . $controllerClass,
        '{',
        '    public function __construct(',
        '        private \\' . $entryFqn . ' $handler,',
        '    ) {',
        '    }',
        '',
        '    public function ' . $action . '(Request $request): Response',
        '    {',
    ];
    // The decode: the typed request binding, or the explicit query
    // input constructed directly.
    if ($route['request'] !== null) {
        $lines[] = '        try {';
        $lines[] = '            $input = \\' . (string) $route['request']['fqn'] . '::fromWire($request);';
        $lines[] = '        } catch (\\Lekalo\\Generated\\Routes\\RequestValidationError $failure) {';
        $lines[] = '            return response()->json(';
        $lines[] = '                \\Lekalo\\Generated\\Routes\\ErrorHttpMap::validation(' . php_types_string_literal($routeId) . ', $failure->fields),';
        $lines[] = '                \\Lekalo\\Generated\\Routes\\ErrorHttpMap::validationStatus(' . php_types_string_literal($routeId) . '),';
        $lines[] = '            );';
        $lines[] = '        }';
    } else {
        $lines[] = '        $input = new \\' . $inputFqn . '();';
    }
    $lines[] = '        $actor = \\Lekalo\\Generated\\Routes\\HttpEnvelope::actor($request, ' . ($authRequired ? 'true' : 'false') . ');';
    $lines[] = '        try {';
    $lines[] = $isCommand
        ? '            $this->handler->handle($input, $actor);'
        : '            $result = $this->handler->handle($input, $actor);';
    foreach ($route['errors'] as $error) {
        if (($error['refusal'] ?? false) === true) {
            // The validation-category error maps through the request
            // binding refusal, not a handler catch.
            continue;
        }
        $errorClass = php_routes_error_class_of((string) $error['error'], $errorBase);
        $payloadArgs = [];
        foreach ($error['payload'] as $member) {
            $type = php_routes_field_type($member, $context['definitions'][$route['operation']], $context['typesIndex'], $context['definitions']);
            $property = php_types_property_of((string) $member);
            $payloadArgs[] = php_types_string_literal((string) $member) . ' => ' . php_routes_wire_expr($type, '$input->' . $property);
        }
        $lines[] = '        } catch (' . $errorClass . ' $failure) {';
        $lines[] = '            return response()->json(';
        $lines[] = '                \\Lekalo\\Generated\\Routes\\ErrorHttpMap::domain(' . php_types_string_literal($routeId) . ', ' . php_types_string_literal((string) $error['error']) . ', [' . implode(', ', $payloadArgs) . ']),';
        $lines[] = '                \\Lekalo\\Generated\\Routes\\ErrorHttpMap::status(' . php_types_string_literal($routeId) . ', ' . php_types_string_literal((string) $error['error']) . '),';
        $lines[] = '            );';
    }
    $lines[] = '        }';
    if ($isCommand) {
        $lines[] = '';
        $lines[] = '        return new \\Illuminate\\Http\\Response(\'\', ' . (int) $route['success']['status'] . ');';
    } else {
        // A list-return query encodes through its declared #58 result
        // codec (issue #50): the collection class carries no wire
        // spelling of its own, the codec is the projection.
        $resultCodecFqn = php_routes_result_codec_fqn($route, $context);
        if ($resultCodecFqn !== null) {
            $wire = '\\' . $resultCodecFqn . '::encode($result)';
        } else {
            $resultType = php_routes_result_type($route, $context);
            $wire = $resultType === null ? 'null' : php_routes_wire_expr($resultType, '$result');
        }
        $lines[] = '';
        $lines[] = '        return response()->json(' . $wire . ', ' . (int) $route['success']['status'] . ');';
    }
    $lines[] = '    }';
    $lines[] = '}';
    return implode("\n", $lines) . "\n";
}

/**
 * The typed error class FQN of one declared error id, spelled exactly
 * as the operations family names its typed error classes.
 */
function php_routes_error_class_of(string $errorId, string $operationsPrefix): string
{
    $module = php_types_module_of($errorId);
    $leaf = php_types_leaf_name_of($errorId);
    $parts = explode('_', $leaf);
    $class = implode('', array_map('ucfirst', array_filter($parts, static fn (string $part): bool => $part !== ''))) . 'Error';
    return '\\' . $operationsPrefix . '\\' . ucfirst($module) . '\\Errors\\' . $class;
}

/** The mapped result type of one query route, when it projects a body. */
function php_routes_result_type(array $route, array $context): ?array
{
    $definition = is_array($context['definitions'][$route['operation']] ?? null) ? $context['definitions'][$route['operation']] : null;
    $ref = $definition === null ? null : ($definition['returns']['ref'] ?? null);
    if (!is_string($ref)) {
        return null;
    }
    $entry = is_array($context['typesIndex'][$ref] ?? null) ? $context['typesIndex'][$ref] : null;
    $type = is_array($context['definitions'][$ref] ?? null) ? $context['definitions'][$ref] : null;
    if ($entry === null || $type === null) {
        return null;
    }
    return ['ref' => $ref, 'fqn' => (string) $entry['fqn'], 'kind' => (string) ($type['kind'] ?? ''), 'base' => (string) ($type['base'] ?? '')];
}

/**
 * The #58 result codec of one list-return query route (issue #50): the
 * mapped query entry is the codec; a scalar-ref query or a command
 * carries none.
 */
function php_routes_result_codec_fqn(array $route, array $context): ?string
{
    $definition = is_array($context['definitions'][$route['operation']] ?? null) ? $context['definitions'][$route['operation']] : null;
    if ($definition === null || !isset($definition['returns']['list']['ref'])) {
        return null;
    }
    $entry = is_array($context['typesIndex'][$route['operation']] ?? null) ? $context['typesIndex'][$route['operation']] : null;
    if ($entry === null || !is_string($entry['fqn'] ?? null)) {
        return null;
    }
    return (string) $entry['fqn'];
}

/** The route registrations text: one block per managed route, id-sorted. */
function php_routes_registrations_text(array $context, array $routes): string
{
    $managed = array_values(array_filter($routes, static fn (array $route): bool => $route['mode'] === 'managed'));
    usort($managed, static fn (array $left, array $right): int => strcmp((string) $left['id'], (string) $right['id']));
    $lines = [
        '<?php',
        '',
        'declare(strict_types=1);',
        '',
        '// Generated by ' . ADAPTER_ID . '@' . ADAPTER_VERSION . ' (routes generator, issue #60).',
        '// From dev.lekalo.ir@0.2.16 input ' . $context['irDigest'] . '.',
        '// Do not edit: regenerate with `lekalo generate`.',
        '//',
        '// The managed route registrations of one routes input. This file is',
        '// the whole registration surface of the routes family: manual routes',
        '// outside it are never touched, and the application merges by',
        '// requiring this file from its own route bootstrap.',
        '',
        'use Illuminate\\Support\\Facades\\Route;',
    ];
    $uses = [];
    foreach ($managed as $route) {
        $uses[] = (string) $route['controller']['fqn'];
    }
    sort($uses, SORT_STRING);
    foreach ($uses as $use) {
        $lines[] = 'use ' . $use . ';';
    }
    $lines[] = '';
    foreach ($managed as $route) {
        $method = strtolower((string) $route['method']);
        $chain = [
            'Route::' . $method . '(' . php_types_string_literal((string) $route['pathTemplate']) . ', ['
                . (string) $route['controller']['fqn'] . '::class, ' . php_types_string_literal((string) $route['controller']['action']) . '])',
            '    ->name(' . php_types_string_literal((string) $route['name']) . ')',
        ];
        if (($route['middleware'] ?? []) !== []) {
            $spellings = implode(', ', array_map(
                static fn (string $spelling): string => php_types_string_literal($spelling),
                $route['middleware'],
            ));
            $chain[] = '    ->middleware([' . $spellings . '])';
        }
        $lines[] = implode("\n", $chain) . ';';
    }
    return implode("\n", $lines) . "\n";
}

/** The deterministic classmap text: FQN => root-relative path. */
function php_routes_classmap_text(array $files, string $root): string
{
    $entries = [];
    foreach ($files as $file) {
        if (!isset($file['fqn'])) {
            continue;
        }
        $relative = substr((string) $file['path'], strlen($root) + 1);
        $entries[(string) $file['fqn']] = $relative;
    }
    ksort($entries);
    $lines = [
        '<?php',
        '',
        'declare(strict_types=1);',
        '',
        '// Generated by ' . ADAPTER_ID . '@' . ADAPTER_VERSION . ' (routes generator, issue #60).',
        '// The deterministic class map: fully-qualified name => root-relative path.',
        '// The loading authority of the generated routes tree - no runtime',
        '// registration magic, no Composer scan.',
        '',
        'return [',
    ];
    foreach ($entries as $fqn => $relative) {
        $lines[] = '    ' . php_types_string_literal($fqn) . ' => ' . php_types_string_literal($relative) . ',';
    }
    $lines[] = '];';
    return implode("\n", $lines) . "\n";
}

/** The canonical custody sidecar over the complete routes inventory. */
function php_routes_sidecar_text(string $projectId, array $mapped, array $files, array $context): string
{
    $routes = [];
    foreach ($mapped['routes'] as $route) {
        $entry = [
            'id' => $route['id'],
            'operation' => $route['operation'],
            'operationId' => $route['operationId'],
            'mode' => $route['mode'],
            'method' => $route['method'],
            'pathTemplate' => $route['pathTemplate'],
            'name' => $route['name'],
        ];
        if ($route['mode'] === 'managed') {
            $entry['controller'] = [
                'fqn' => $route['controller']['fqn'],
                'path' => $route['controller']['path'],
                'action' => $route['controller']['action'],
            ];
            if ($route['request'] !== null) {
                $entry['request'] = ['fqn' => $route['request']['fqn'], 'path' => $route['request']['path']];
            }
        }
        $entry['entry'] = ['fqn' => $route['entry']['fqn'], 'method' => $route['entry']['method']];
        $entry['middleware'] = $route['middleware'];
        $success = ['status' => $route['success']['status']];
        if ($route['success']['bodyMode'] !== null) {
            $success['bodyMode'] = $route['success']['bodyMode'];
        }
        $entry['success'] = $success;
        $entry['errors'] = array_map(
            static fn (array $error): array => ['error' => $error['error'], 'status' => $error['status']],
            $route['errors'],
        );
        $entry['errorDefaults'] = $route['errorDefaults'];
        if ($route['auth'] !== null) {
            $entry['auth'] = array_filter([
                'actor' => $route['auth']['actor'],
                'schemes' => $route['auth']['schemes'],
                'policyRef' => $route['auth']['policyRef'],
            ], static fn (mixed $value): bool => $value !== null);
        }
        $entry['links'] = $route['links'];
        $routes[] = $entry;
    }
    $artifacts = [];
    foreach ($files as $file) {
        $row = [
            'path' => substr((string) $file['path'], strlen(PHP_ROUTES_GENERATED_ROOT) + 1),
            'role' => $file['role'],
            'lifecycle' => $file['lifecycle'],
        ];
        if (isset($file['fqn'])) {
            $row['fqn'] = $file['fqn'];
        }
        $row['digest'] = $file['digest'];
        $artifacts[] = $row;
    }
    usort($artifacts, static fn (array $left, array $right): int => strcmp((string) $left['path'], (string) $right['path']));
    $document = [
        'schemaVersion' => PHP_ROUTES_MAP_SCHEMA_VERSION,
        'identity' => PHP_ROUTES_MAP_IDENTITY,
        'projectId' => $projectId,
        'adapter' => [
            'id' => ADAPTER_ID,
            'version' => ADAPTER_VERSION,
            'digest' => sha256_digest(ADAPTER_ID . '@' . ADAPTER_VERSION),
        ],
        'digests' => [
            'ir' => $context['irDigest'],
            'transport' => $context['transportDigest'],
            'input' => $context['inputDigest'],
            'typesInput' => $context['typesInputDigest'],
            'operationsInput' => $context['operationsInputDigest'],
            'openapi' => $context['openapiDigest'],
        ],
        'routes' => $routes,
        'artifacts' => $artifacts,
    ];
    return php_types_canonical_json($document) . "\n";
}


// ---- bundled Composer gates policy (issue #61) ------------------
// EMBEDDED BY build.php from composer-gates-policy.json. Never edit.
// Confirmation data only — never execution authority. The exact
// bytes are part of this artifact (upgrade custody binds).
const NATIVE_GATES_POLICY_BUNDLED = '{
    "schema_version": "lekalo/native-gate-policy/v0.4.0",
    "kind": "native-gate-policy",
    "policy_digest": "sha256:ef4ba2a935916f6ba74baadc1b83908ca99692bcad82c8a009af47ec6904c912",
    "identity": {
        "id": "composer-gates-policy",
        "version": "0.4.0"
    },
    "repository_role": "consumer-repository",
    "trust": {
        "mode": "public-fixture",
        "fixture_ref": "sha256:437d90ae3f40629e7cc91bad399bc1680ea587e0154044e03641cfba9eb21b3b",
        "provenance_ref": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    },
    "authority_ref": {
        "contractId": "dev.lekalo.authority-matrix",
        "version": "0.3.2",
        "digest": "sha256:cf60a50f9df62df54728fab319e1b0e139208f820ec8c82d757bfe853c0f03b4"
    },
    "policy_ref": {
        "policyId": "dev.lekalo.privacy-export-policy",
        "version": "0.3.2",
        "digest": "sha256:5643547b96e1ca9f422e91e699c8d04e676e6eb820b4ef21a88860c74133c3b9"
    },
    "classification_ref": {
        "contractId": "dev.lekalo.privacy-classification-decision",
        "version": "0.2.16",
        "digest": "sha256:78de535f02b6a8065579b43798ed650849aca0b7edf1aa08b2e0219fc744dde6"
    },
    "allowed_gate_kinds": [
        "build",
        "typecheck",
        "lint",
        "test"
    ],
    "confirmations": [
        {
            "package_id": ".=lekalo/planner-laravel-fixture",
            "gate": "lint",
            "gate_id": "mago-format",
            "gate_kind": "mago-format",
            "required": false,
            "cwd": ".",
            "script_name": "gate:format",
            "manifest_digest": "sha256:437d90ae3f40629e7cc91bad399bc1680ea587e0154044e03641cfba9eb21b3b",
            "script_digest": "sha256:d6c4b314078d1c8acc96a682b8def6a39e3520e4fd8befd3038322f6f7a965c4",
            "argv": [
                "php",
                "vendor/bin/mago",
                "format",
                "--check"
            ],
            "tool_ref": {
                "id": "php-runtime",
                "artifact_digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "entry_digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "version": "8.3"
            },
            "covers_suite_ids": [],
            "rule_version": "0.4.0",
            "rule_digest": "sha256:1111111111111111111111111111111111111111111111111111111111111101"
        },
        {
            "package_id": ".=lekalo/planner-laravel-fixture",
            "gate": "lint",
            "gate_id": "mago-lint",
            "gate_kind": "mago-lint",
            "required": true,
            "cwd": ".",
            "script_name": "gate:lint",
            "manifest_digest": "sha256:437d90ae3f40629e7cc91bad399bc1680ea587e0154044e03641cfba9eb21b3b",
            "script_digest": "sha256:17c1cf4e467d7dc770b5851fd7b916b75009f73ec10d14b2f26ef26ecc17fc5d",
            "argv": [
                "php",
                "vendor/bin/mago",
                "lint"
            ],
            "tool_ref": {
                "id": "php-runtime",
                "artifact_digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "entry_digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "version": "8.3"
            },
            "covers_suite_ids": [],
            "rule_version": "0.4.0",
            "rule_digest": "sha256:1111111111111111111111111111111111111111111111111111111111111102"
        },
        {
            "package_id": ".=lekalo/planner-laravel-fixture",
            "gate": "typecheck",
            "gate_id": "mago-analyze",
            "gate_kind": "mago-analyze",
            "required": true,
            "cwd": ".",
            "script_name": "gate:analyze",
            "manifest_digest": "sha256:437d90ae3f40629e7cc91bad399bc1680ea587e0154044e03641cfba9eb21b3b",
            "script_digest": "sha256:7246fefdb2de6850cdb0498ed9f806255751e493d0d0393dbd983ed16dc6c16e",
            "argv": [
                "php",
                "vendor/bin/mago",
                "analyze"
            ],
            "tool_ref": {
                "id": "php-runtime",
                "artifact_digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "entry_digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "version": "8.3"
            },
            "covers_suite_ids": [],
            "rule_version": "0.4.0",
            "rule_digest": "sha256:1111111111111111111111111111111111111111111111111111111111111103"
        },
        {
            "package_id": ".=lekalo/planner-laravel-fixture",
            "gate": "test",
            "gate_id": "scenario-tests",
            "gate_kind": "laratesto",
            "required": true,
            "cwd": ".",
            "script_name": "gate:scenario-tests",
            "manifest_digest": "sha256:437d90ae3f40629e7cc91bad399bc1680ea587e0154044e03641cfba9eb21b3b",
            "script_digest": "sha256:03092147fdea7fd034b124bad3495a7dd72af5e8ebbc1b00e20cd7270ded0c6b",
            "argv": [
                "php",
                "vendor/bin/testo",
                "run",
                "--config",
                "testo.php",
                "--suite=Laravel"
            ],
            "tool_ref": {
                "id": "php-runtime",
                "artifact_digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "entry_digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "version": "8.3"
            },
            "covers_suite_ids": [
                "laravel"
            ],
            "rule_version": "0.4.0",
            "rule_digest": "sha256:1111111111111111111111111111111111111111111111111111111111111104"
        },
        {
            "package_id": ".=lekalo/planner-laravel-fixture",
            "gate": "test",
            "gate_id": "legacy-tests",
            "gate_kind": "legacy-suite",
            "required": false,
            "cwd": ".",
            "script_name": "gate:legacy-tests",
            "manifest_digest": "sha256:437d90ae3f40629e7cc91bad399bc1680ea587e0154044e03641cfba9eb21b3b",
            "script_digest": "sha256:4344f25bc05b49af1b9d63b58f40705699af1b8363d8d4272b4bf67d12bbe71d",
            "argv": [
                "php",
                "vendor/bin/phpunit",
                "--config",
                "tests/legacy/phpunit.xml"
            ],
            "tool_ref": {
                "id": "php-runtime",
                "artifact_digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "entry_digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "version": "8.3"
            },
            "covers_suite_ids": [
                "legacy-suite"
            ],
            "rule_version": "0.4.0",
            "rule_digest": "sha256:1111111111111111111111111111111111111111111111111111111111111105"
        },
        {
            "package_id": ".=lekalo/planner-laravel-fixture",
            "gate": "test",
            "gate_id": "testo-full",
            "gate_kind": "laratesto",
            "required": true,
            "cwd": ".",
            "script_name": "gate:testo-full",
            "manifest_digest": "sha256:437d90ae3f40629e7cc91bad399bc1680ea587e0154044e03641cfba9eb21b3b",
            "script_digest": "sha256:39c1fb6c6931b69ea41b4ff029a0112a49a1966239715d8f199ce5033121944d",
            "argv": [
                "php",
                "vendor/bin/testo",
                "run",
                "--config",
                "testo.php"
            ],
            "tool_ref": {
                "id": "php-runtime",
                "artifact_digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "entry_digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "version": "8.3"
            },
            "covers_suite_ids": [
                "laravel",
                "legacy-suite"
            ],
            "rule_version": "0.4.0",
            "rule_digest": "sha256:1111111111111111111111111111111111111111111111111111111111111106"
        },
        {
            "package_id": ".=lekalo/planner-laravel-fixture",
            "gate": "build",
            "gate_id": "application-boot",
            "gate_kind": "boot-smoke",
            "required": true,
            "cwd": ".",
            "script_name": "gate:boot",
            "manifest_digest": "sha256:437d90ae3f40629e7cc91bad399bc1680ea587e0154044e03641cfba9eb21b3b",
            "script_digest": "sha256:aaaabe1384ac3bc152818cc12f4c4e92190f5bb8b28b152d4a67ca36bbe31684",
            "argv": [
                "php",
                "artisan",
                "about"
            ],
            "tool_ref": {
                "id": "php-runtime",
                "artifact_digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "entry_digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "version": "8.3"
            },
            "covers_suite_ids": [],
            "rule_version": "0.4.0",
            "rule_digest": "sha256:1111111111111111111111111111111111111111111111111111111111111107"
        },
        {
            "package_id": ".=lekalo/planner-laravel-fixture",
            "gate": "build",
            "gate_id": "migration-static",
            "gate_kind": "migration-static",
            "required": true,
            "cwd": ".",
            "script_name": "gate:migration-static",
            "manifest_digest": "sha256:437d90ae3f40629e7cc91bad399bc1680ea587e0154044e03641cfba9eb21b3b",
            "script_digest": "sha256:f47b1320f987f1e5ada27186ef884cf5cc6c627c34a4115b3232fade684cedef",
            "argv": [
                "php",
                "artisan",
                "migrate:status"
            ],
            "tool_ref": {
                "id": "php-runtime",
                "artifact_digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "entry_digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "version": "8.3"
            },
            "covers_suite_ids": [],
            "rule_version": "0.4.0",
            "rule_digest": "sha256:1111111111111111111111111111111111111111111111111111111111111108"
        }
    ],
    "fallback_rule": {
        "mode": "none"
    },
    "env_recipe": {
        "allowed_names": [],
        "bindings": []
    },
    "limits": {
        "timeout_ms_per_command": 10000,
        "timeout_ms_per_run": 60000,
        "max_stdout_bytes": 65536,
        "max_stderr_bytes": 65536,
        "max_output_bytes_per_run": 1048576
    },
    "write_policy": {
        "mode": "stage-only"
    }
}
';

// ----- native gate module: native-policy.php -----

/**
 * Pure Composer/Laravel execution-policy verification (issue #61, plan
 * slice B; research doc §1 step 3 and §2).
 *
 * `php_verify_confirmations` checks every confirmed (package, gate)
 * recipe of the checked-in execution policy against the project's real
 * `composer.json` bytes: the manifest digest, the exact script entry
 * bytes, the decoded literal argv, and the bounded transitive recipe
 * closure. A confirmation never derives authority from being written
 * down — the manifest text is the only source of the executed argv,
 * and the policy argv must match it exactly.
 *
 * Unlike the Node #48 planner's `confirmationByPackage` map, which
 * silently kept only the last confirmation per package, confirmations
 * join per package by the stable `gate_id`: several confirmations for
 * one package each produce their own confirmed tuple.
 *
 * The module is pure: no clock, no environment, no process launch, no
 * I/O — the project bytes arrive through the injected `callable
 * $reader(string $logicalPath): ?string`. `php -n` compatible: PHP
 * built-ins only (json, hash, pcre), no Composer package.
 */


/** The digest domain of the checked-in Composer execution policy. */
const PHP_POLICY_DIGEST_DOMAIN = 'lekalo.native-policy.v0.4.0';

/** The closed v0.4.0 gate-kind metadata set (mirrors the successor schema). */
const PHP_GATE_KINDS = [
    'composer-script', 'mago-format', 'mago-lint', 'mago-analyze', 'mago-guard',
    'laratesto', 'pest', 'phpunit', 'artisan-check', 'boot-smoke',
    'migration-static', 'migration-execute', 'discovery-smoke', 'legacy-suite',
];

/** The closed execution-gate set (the four-value v0.3.2 enum, unchanged). */
const PHP_GATES = ['build', 'typecheck', 'lint', 'test'];

/** Shell metacharacters and interpolation syntax refused in script literals. */
const PHP_SHELL_METACHARACTERS = '|&;<>()$`"\'\\' . "%\n\r\t";

/** Composer/PHP subcommands that mutate, resolve the network, or dispatch plugins. */
const PHP_FORBIDDEN_SCRIPT_TOKENS = [
    'composer', 'composer.phar', 'install', 'update', 'require', 'remove',
    'config', 'global', 'create-project', 'exec', 'diagnose', 'putenv',
    'sh', 'bash', 'cmd', 'powershell', 'pwsh', 'eval',
];

/** Artisan commands that open sockets or interactive sessions. */
const PHP_FORBIDDEN_ARTISAN_TOKENS = ['serve', 'tinker', 'rx'];

/** The maximum scripts one composer.json may declare (decoder bound). */
const PHP_MAX_SCRIPTS = 64;
/** The maximum elements one script entry may carry (decoder bound). */
const PHP_MAX_SCRIPT_ELEMENTS = 16;
/** The maximum resolved leaf commands one script closure may expand to. */
const PHP_MAX_CLOSURE_COMMANDS = 128;
/** The maximum bytes of one script entry string. */
const PHP_MAX_SCRIPT_BYTES = 4096;

/**
 * Whether one string is a safe literal argv element: no shell
 * metacharacters, no interpolation, no globs. The check is on the
 * complete string, never a prefix.
 */
function php_is_safe_literal(string $text): bool
{
    if ($text === '' || strlen($text) > 1024) {
        return false;
    }
    if ((bool) preg_match('/[\s]{2,}/', $text)) {
        return false;
    }
    for ($i = 0, $n = strlen($text); $i < $n; $i++) {
        if (str_contains(PHP_SHELL_METACHARACTERS, $text[$i])) {
            return false;
        }
    }
    return true;
}

/**
 * Canonical JSON text: recursively bytewise key-sorted, compact, UTF-8.
 * Delegates to the kernel's byte-compatible encoder when loaded (the
 * shipped artifact concatenates the kernel first); standalone test
 * loading falls back to the same closed encoder inline.
 */
function php_canonical_text(array|bool|int|string|null $value): string
{
    if (function_exists('write_canonical')) {
        return write_canonical($value);
    }
    if ($value === null) {
        return 'null';
    }
    if (is_bool($value)) {
        return $value ? 'true' : 'false';
    }
    if (is_int($value)) {
        return (string) $value;
    }
    if (is_string($value)) {
        $encoded = json_encode($value, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR);
        return $encoded;
    }
    if (array_is_list($value)) {
        return '[' . implode(',', array_map(__FUNCTION__, $value)) . ']';
    }
    $keys = array_keys($value);
    usort($keys, 'strcmp');
    $body = [];
    foreach ($keys as $key) {
        $body[] = json_encode((string) $key, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR)
            . ':' . php_canonical_text($value[$key]);
    }
    return '{' . implode(',', $body) . '}';
}

/** sha256 over the pinned domain joined with the canonical bytes. */
function php_domain_digest(string $domain, array|bool|int|string|null $value): string
{
    return 'sha256:' . hash('sha256', $domain . php_canonical_text($value));
}

/**
 * Decode one composer.json `scripts` entry into the resolved literal
 * command list. Returns ['ok' => true, 'commands' => string[][]] where
 * each command is an argv vector, or ['ok' => false, 'reason' => token].
 *
 * Accepted forms (research §2: a simple direct literal recipe first;
 * anything else stays unsupported without substituting scripts):
 *   - "literal command string"       one command, single-spaced tokens;
 *   - "@other-script"                a reference edge into the DAG;
 *   - ["@a", "literal", ["extra"]]   element list; an array element
 *                                    appends literal args to the
 *                                    previous command.
 * `@php` resolves to the pinned interpreter token `php`; `@composer`
 * and every other Composer dispatch form is refused — the host never
 * launches the Composer dispatcher, installs, updates, or resolves
 * the network (research §3 step 3).
 *
 * @param array<string, mixed> $scripts the decoded scripts map
 * @param string $entry the requested script name
 */
function php_decode_composer_script(array $scripts, string $entry): array
{
    if ($entry === '' || strlen($entry) > 64
        || !(bool) preg_match('/^[A-Za-z0-9_][A-Za-z0-9._:-]*$/', $entry)) {
        return ['ok' => false, 'reason' => 'script-name-invalid'];
    }
    if (!array_key_exists($entry, $scripts)) {
        return ['ok' => false, 'reason' => 'script-absent'];
    }
    if (count($scripts) > PHP_MAX_SCRIPTS) {
        return ['ok' => false, 'reason' => 'script-map-oversize'];
    }
    $resolved = [];
    $stack = [];
    $outcome = php_decode_script_value($scripts[$entry], $scripts, $entry, $resolved, $stack, 0);
    if (!$outcome['ok']) {
        return $outcome;
    }
    if (count($resolved) === 0 || count($resolved) > PHP_MAX_CLOSURE_COMMANDS) {
        return ['ok' => false, 'reason' => 'script-closure-bound'];
    }
    // Every resolved leaf command must be a literal argv whose
    // executable is the pinned interpreter token and whose program is
    // a repo-relative path: the host resolves `php` through the
    // trusted catalog, never ambient PATH or a project shim.
    foreach ($resolved as $argv) {
        if ($argv[0] !== 'php') {
            return ['ok' => false, 'reason' => 'script-executable-unsupported'];
        }
        $program = $argv[1] ?? '';
        if ($program === '' || str_starts_with($program, '/')
            || str_contains($program, '\\') || str_contains($program, '..')) {
            return ['ok' => false, 'reason' => 'script-program-path'];
        }
    }
    return ['ok' => true, 'commands' => $resolved];
}

/**
 * Recursive decoder for one script value. `$resolved` collects the
 * leaf argv vectors; `$stack` carries the active reference path for
 * cycle rejection.
 *
 * @param array<string, mixed> $scripts
 * @param list<list<string>> $resolved
 * @param list<string> $stack
 */
function php_decode_script_value(mixed $value, array $scripts, string $name, array &$resolved, array &$stack, int $depth): array
{
    if ($depth > PHP_MAX_SCRIPTS) {
        return ['ok' => false, 'reason' => 'script-cycle'];
    }
    if (is_string($value)) {
        return php_decode_script_string($value, $scripts, $name, $resolved, $stack, $depth);
    }
    if (is_array($value) && array_is_list($value)) {
        if (count($value) === 0 || count($value) > PHP_MAX_SCRIPT_ELEMENTS) {
            return ['ok' => false, 'reason' => 'script-element-bound'];
        }
        $appendedTo = -1;
        foreach ($value as $element) {
            if (is_array($element)) {
                // An array element appends literal arguments to the
                // previous command (Composer arg-list semantics).
                if ($appendedTo < 0) {
                    return ['ok' => false, 'reason' => 'script-unsupported'];
                }
                if (count($element) > PHP_MAX_SCRIPT_ELEMENTS) {
                    return ['ok' => false, 'reason' => 'script-element-bound'];
                }
                foreach ($element as $argument) {
                    if (!is_string($argument) || !php_is_safe_literal($argument)) {
                        return ['ok' => false, 'reason' => 'script-unsafe-element'];
                    }
                    if (php_token_forbidden($argument)) {
                        return ['ok' => false, 'reason' => 'script-package-manager'];
                    }
                    $resolved[$appendedTo][] = $argument;
                }
                continue;
            }
            if (!is_string($element)) {
                return ['ok' => false, 'reason' => 'script-unsupported'];
            }
            $before = count($resolved);
            $outcome = php_decode_script_string($element, $scripts, $name, $resolved, $stack, $depth);
            if (!$outcome['ok']) {
                return $outcome;
            }
            // A plain command element starts a new command; a pure
            // reference to a previous element extends it.
            $appendedTo = count($resolved) - 1;
            if (count($resolved) === $before && $appendedTo >= 0) {
                $appendedTo = $before - 1;
            }
        }
        return ['ok' => true];
    }
    return ['ok' => false, 'reason' => 'script-unsupported'];
}

/**
 * One command string: a pure `@reference`, or a single-spaced literal
 * command whose tokens survive the closed safety grammar.
 *
 * @param array<string, mixed> $scripts
 * @param list<list<string>> $resolved
 * @param list<string> $stack
 */
function php_decode_script_string(string $text, array $scripts, string $name, array &$resolved, array &$stack, int $depth): array
{
    if ($text === '' || strlen($text) > PHP_MAX_SCRIPT_BYTES || !php_is_safe_literal($text)) {
        return ['ok' => false, 'reason' => 'script-shell-syntax'];
    }
    $tokens = explode(' ', $text);
    $first = $tokens[0];
    if (str_starts_with($first, '@')) {
        $reference = substr($first, 1);
        if ($reference === '') {
            return ['ok' => false, 'reason' => 'script-reference-empty'];
        }
        if ($reference === 'composer') {
            // Composer dispatch: never planned, never launched.
            return ['ok' => false, 'reason' => 'script-package-manager'];
        }
        if ($reference === 'php') {
            // The Composer-registered interpreter alias resolves to
            // the pinned interpreter token; the rest are literals and
            // must survive the same closed token grammar.
            if (count($tokens) < 2) {
                return ['ok' => false, 'reason' => 'script-unsupported'];
            }
            array_shift($tokens);
            foreach ($tokens as $token) {
                if (php_token_forbidden($token)) {
                    return ['ok' => false, 'reason' => 'script-package-manager'];
                }
            }
            // The confirmed-program invariant: argv[1] is a repo-
            // relative file the host stages and digests. A flag-shaped
            // leaf (@php -r ..., @php --version) would hand the pinned
            // interpreter an inline program, so it is refused.
            if (str_starts_with($tokens[0], '-')) {
                return ['ok' => false, 'reason' => 'script-program-path'];
            }
            if ($tokens[0] === 'artisan' && php_artisan_token_forbidden($tokens)) {
                return ['ok' => false, 'reason' => 'script-network-or-interactive'];
            }
            $resolved[] = ['php', ...$tokens];
            return ['ok' => true];
        }
        if (isset($scripts[$reference])) {
            // A reference to another script: DAG edge with cycle
            // rejection over the active stack.
            if (in_array($reference, $stack, true)) {
                return ['ok' => false, 'reason' => 'script-cycle'];
            }
            if (count($tokens) !== 1) {
                return ['ok' => false, 'reason' => 'script-reference-args'];
            }
            $stack[] = $reference;
            $outcome = php_decode_script_value($scripts[$reference], $scripts, $reference, $resolved, $stack, $depth + 1);
            array_pop($stack);
            return $outcome;
        }
        return ['ok' => false, 'reason' => 'script-reference-unknown'];
    }
    if (count($tokens) > 0 && $tokens[0] === 'composer') {
        return ['ok' => false, 'reason' => 'script-package-manager'];
    }
    if ($tokens[0] === 'php' && ($tokens[1] ?? '') === 'artisan'
        && php_artisan_token_forbidden($tokens)) {
        return ['ok' => false, 'reason' => 'script-network-or-interactive'];
    }
    foreach ($tokens as $token) {
        if (php_token_forbidden($token)) {
            return ['ok' => false, 'reason' => 'script-package-manager'];
        }
    }
    $resolved[] = $tokens;
    return ['ok' => true];
}

/** Whether one literal token names a mutating or dispatching program. */
function php_token_forbidden(string $token): bool
{
    $lower = strtolower($token);
    if (in_array($lower, PHP_FORBIDDEN_SCRIPT_TOKENS, true)) {
        return true;
    }
    if (str_ends_with($lower, '/composer') || str_ends_with($lower, '\\composer')
        || str_ends_with($lower, 'composer.phar')) {
        return true;
    }
    // Assignment prefixes are environment injection through the shell.
    if ((bool) preg_match('/^[A-Za-z_][A-Za-z0-9_]*=/', $token)) {
        return true;
    }
    return false;
}

/** Whether one `php artisan ...` argv carries a networked/interactive command. */
function php_artisan_token_forbidden(array $tokens): bool
{
    foreach ($tokens as $token) {
        if (in_array(strtolower($token), PHP_FORBIDDEN_ARTISAN_TOKENS, true)) {
            return true;
        }
    }
    return false;
}

/**
 * Verify every confirmation of the execution policy against the
 * project's real composer.json bytes, joining several confirmations
 * per package by the stable gate id.
 *
 * `$reader(string $logicalPath): ?string` supplies the project bytes
 * (null when the file is absent or outside the declared read view).
 * The returned shape:
 *   ['ok' => bool, 'unverifiable' => list<string>, 'confirmed' => list<array>,
 *    'tool_catalog' => list<array>, 'policy_digest' => string]
 *
 * @param callable(string): ?string $reader
 */
function php_verify_confirmations(array $policyDocument, callable $reader): array
{
    $unverifiable = [];
    // The policy digest is computed over the canonical document with
    // the digest member absent (the plan_digest convention).
    $digestInput = $policyDocument;
    unset($digestInput['policy_digest']);
    $policyDigest = php_domain_digest(PHP_POLICY_DIGEST_DOMAIN, $digestInput);
    if (($policyDocument['policy_digest'] ?? null) !== $policyDigest) {
        $unverifiable[] = 'policy-digest-drift';
        return ['ok' => false, 'unverifiable' => $unverifiable, 'confirmed' => [], 'tool_catalog' => [], 'policy_digest' => $policyDigest];
    }
    $manifestBytes = $reader('composer.json');
    if ($manifestBytes === null) {
        return ['ok' => false, 'unverifiable' => ['composer-manifest-absent'], 'confirmed' => [], 'tool_catalog' => [], 'policy_digest' => $policyDigest];
    }
    $manifestDigest = 'sha256:' . hash('sha256', $manifestBytes);
    try {
        $manifest = json_decode($manifestBytes, true, 64, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        return ['ok' => false, 'unverifiable' => ['composer-manifest-unparsable'], 'confirmed' => [], 'tool_catalog' => [], 'policy_digest' => $policyDigest];
    }
    if (!is_array($manifest) || !isset($manifest['name']) || !is_string($manifest['name'])) {
        return ['ok' => false, 'unverifiable' => ['composer-manifest-invalid'], 'confirmed' => [], 'tool_catalog' => [], 'policy_digest' => $policyDigest];
    }
    $scripts = isset($manifest['scripts']) && is_array($manifest['scripts']) ? $manifest['scripts'] : [];
    $packageId = '.' . '=' . $manifest['name'];
    $confirmed = [];
    foreach (($policyDocument['confirmations'] ?? []) as $confirmation) {
        $joined = php_verify_one_confirmation($confirmation, $packageId, $manifestDigest, $scripts, $reader);
        if ($joined['ok']) {
            $confirmed[] = $joined['confirmed'];
        } else {
            $unverifiable[] = $joined['reason'];
        }
    }
    // The whole policy must verify: a partially confirmed gate set is
    // never a planning input (fail closed, never a silent subset).
    $ok = $unverifiable === [] && $confirmed !== [];
    return [
        'ok' => $ok,
        'unverifiable' => array_slice($unverifiable, 0, 16),
        'confirmed' => $confirmed,
        'tool_catalog' => php_build_tool_catalog($policyDocument),
        'policy_digest' => $policyDigest,
    ];
}

/**
 * Verify one confirmation against the decoded scripts map. The exact
 * script entry bytes are re-canonicalized for the entry digest, the
 * decoded closure must match the confirmed argv command-for-command,
 * and the confirmation's cwd/package/gate-kind must sit inside the
 * closed grammar.
 *
 * @param array<string, mixed> $scripts
 * @param callable(string): ?string $reader
 * @return array{ok: bool, confirmed?: array<string, mixed>, reason?: string}
 */
function php_verify_one_confirmation(array $confirmation, string $packageId, string $manifestDigest, array $scripts, callable $reader): array
{
    foreach (['package_id', 'gate', 'gate_id', 'gate_kind', 'required', 'cwd', 'script_name', 'manifest_digest', 'script_digest', 'argv', 'tool_ref', 'rule_version', 'rule_digest'] as $key) {
        if (!array_key_exists($key, $confirmation)) {
            return ['ok' => false, 'reason' => 'confirmation-incomplete'];
        }
    }
    if ($confirmation['package_id'] !== $packageId) {
        // A confirmation for another project is not applicable here;
        // it is neither verified nor a failure (fixture policies carry
        // several roots' worth of confirmations by design).
        return ['ok' => false, 'reason' => 'confirmation-other-package'];
    }
    if (!in_array($confirmation['gate'], PHP_GATES, true)
        || !in_array($confirmation['gate_kind'], PHP_GATE_KINDS, true)) {
        return ['ok' => false, 'reason' => 'confirmation-gate-unsupported'];
    }
    if (!is_bool($confirmation['required'])) {
        return ['ok' => false, 'reason' => 'confirmation-required-invalid'];
    }
    if ($confirmation['cwd'] !== '.' && !(bool) preg_match('/^[a-z0-9.][a-z0-9._-]*(\/[a-z0-9.][a-z0-9._-]*)*$/', (string) $confirmation['cwd'])) {
        return ['ok' => false, 'reason' => 'confirmation-cwd-invalid'];
    }
    // Custody: the confirmation names the exact manifest bytes this
    // kernel just read — manifest drift is never a silent pass.
    if ($confirmation['manifest_digest'] !== $manifestDigest) {
        return ['ok' => false, 'reason' => 'manifest-digest-drift'];
    }
    $scriptName = (string) $confirmation['script_name'];
    if (!array_key_exists($scriptName, $scripts)) {
        return ['ok' => false, 'reason' => 'script-absent-' . substr($scriptName, 0, 48)];
    }
    $entryDigest = 'sha256:' . hash('sha256', php_canonical_text($scripts[$scriptName]));
    if ($entryDigest !== $confirmation['script_digest']) {
        return ['ok' => false, 'reason' => 'script-digest-drift-' . substr($scriptName, 0, 48)];
    }
    $decoded = php_decode_composer_script($scripts, $scriptName);
    if (!$decoded['ok']) {
        return ['ok' => false, 'reason' => $decoded['reason'] . '-' . substr($scriptName, 0, 48)];
    }
    // One confirmed recipe = one resolved literal command. Multi-
    // command closures are recorded but only a single-command closure
    // confirms a runnable gate today (complex recipes stay unsupported
    // until they have a qualified execution path).
    if (count($decoded['commands']) !== 1) {
        return ['ok' => false, 'reason' => 'script-closure-complex-' . substr($scriptName, 0, 48)];
    }
    $argv = $decoded['commands'][0];
    if (!is_array($confirmation['argv']) || $confirmation['argv'] !== $argv) {
        return ['ok' => false, 'reason' => 'argv-mismatch-' . substr($scriptName, 0, 48)];
    }
    $toolRef = $confirmation['tool_ref'];
    if (!is_array($toolRef) || !isset($toolRef['id'], $toolRef['artifact_digest'])
        || !is_string($toolRef['id']) || !is_string($toolRef['artifact_digest'])
        || !(bool) preg_match('/^sha256:[0-9a-f]{64}$/', $toolRef['artifact_digest'])) {
        return ['ok' => false, 'reason' => 'confirmation-tool-invalid'];
    }
    $covers = $confirmation['covers_suite_ids'] ?? [];
    if (!is_array($covers)) {
        return ['ok' => false, 'reason' => 'confirmation-covers-invalid'];
    }
    $transitive = php_script_closure_digest($scripts, $scriptName);
    return ['ok' => true, 'confirmed' => [
        'package_id' => $confirmation['package_id'],
        'gate' => $confirmation['gate'],
        'gate_id' => (string) $confirmation['gate_id'],
        'gate_kind' => (string) $confirmation['gate_kind'],
        'required' => $confirmation['required'],
        'cwd' => (string) $confirmation['cwd'],
        'script_name' => $scriptName,
        'manifest_digest' => (string) $confirmation['manifest_digest'],
        'script_digest' => (string) $confirmation['script_digest'],
        'closure_digest' => $transitive,
        'confirmation_ref' => (string) $confirmation['rule_digest'],
        'argv' => $argv,
        'env' => [],
        'tool_ref' => (string) $toolRef['id'],
        'covers_suite_ids' => array_values(array_filter($covers, 'is_string')),
    ]];
}

/**
 * The bounded transitive recipe custody digest: sha256 over the
 * canonical resolved closure of one script (references expanded,
 * cycles refused). The runner rechecks this digest before launch.
 */
function php_script_closure_digest(array $scripts, string $scriptName): string
{
    $decoded = php_decode_composer_script($scripts, $scriptName);
    if (!$decoded['ok']) {
        return 'sha256:' . hash('sha256', 'unresolved:' . $decoded['reason']);
    }
    return php_domain_digest('lekalo.native-script-closure.v0.4.0', $decoded['commands']);
}

/**
 * The trusted tool catalog from the policy's confirmed tool refs,
 * deduplicated by id: the digests are the custody anchors the host
 * runner verifies before any launch (never PATH, never a .bat shim).
 *
 * @return list<array<string, mixed>>
 */
function php_build_tool_catalog(array $policyDocument): array
{
    $seen = [];
    $catalog = [];
    foreach (($policyDocument['confirmations'] ?? []) as $confirmation) {
        $tool = $confirmation['tool_ref'] ?? null;
        if (!is_array($tool) || !isset($tool['id']) || !is_string($tool['id']) || isset($seen[$tool['id']])) {
            continue;
        }
        $seen[$tool['id']] = true;
        $catalog[] = [
            'id' => $tool['id'],
            'name' => is_array($confirmation['argv'] ?? null) ? (string) ($confirmation['argv'][0] ?? $tool['id']) : $tool['id'],
            'version' => isset($tool['version']) && is_string($tool['version']) ? $tool['version'] : 'unknown',
            'artifact_digest' => (string) $tool['artifact_digest'],
            'entry_digest' => isset($tool['entry_digest']) && is_string($tool['entry_digest']) ? $tool['entry_digest'] : null,
            'platform' => PHP_OS_FAMILY === 'Windows' ? 'windows' : strtolower(PHP_OS_FAMILY),
            'provenance' => 'checked-in-policy',
        ];
    }
    usort($catalog, static fn (array $left, array $right): int => strcmp($left['id'], $right['id']));
    return $catalog;
}

/**
 * The tool catalog digest: sha256 over the canonical catalog bytes.
 */
function php_tool_catalog_digest(array $catalog): string
{
    return php_domain_digest('lekalo.native-tool-catalog.v0.4.0', $catalog);
}

// ----- native gate module: native-plan.php -----

/**
 * Pure Composer/Laravel native gate plan construction (issue #61, plan
 * slice B; research doc §1 step 2 and §4).
 *
 * `php_build_native_plan` joins the verified Composer metadata (the
 * manifest bytes already checked by `php_verify_confirmations`), the
 * confirmed (package, gate, cwd, argv, digests) tuples, the host-
 * validated selection manifest, and the bounded changed inputs into
 * one immutable proposed `lekalo/native-gate-plan/v0.4.0` document.
 * It is a pure function of its inputs: no clock, no environment, no
 * absolute paths, no randomness, no I/O, no process launch.
 *
 * The plan is a proposal only — it can never authorize its own
 * execution, and the approval always lives outside the plan. The
 * canonical digest is SHA-256 over `domain || canonical(plan)` with
 * the canonical form UTF-8 JSON with recursively bytewise-sorted keys
 * and compact separators, `plan_digest` absent — byte-compatible with
 * the Rust core and the Node planner (shared golden vectors).
 */


const PHP_PLAN_DIGEST_DOMAIN = 'lekalo.native-plan.v0.4.0';
const PHP_PLAN_SCHEMA_VERSION = 'lekalo/native-gate-plan/v0.4.0';
const PHP_SELECTION_DIGEST_DOMAIN = 'lekalo.native-selection.v0.4.0';
/** The planner capability id declared in every produced plan. */
const PHP_PLAN_CAPABILITY = 'plan.native-gates';
const PHP_PLANNER_VERSION = '0.4.0';
const PHP_CANONICALIZATION_VERSION = '0.4.0';

/** Plan bounds (mirrors the Rust wire validator). */
const PHP_MAX_CHANGED_FILES = 1024;
const PHP_MAX_MODULES = 256;
const PHP_MAX_MANDATORY = 128;
const PHP_MAX_COMMANDS = 128;

/**
 * One typed plan refusal with a stable bounded code.
 */
final class PhpPlanRefusal extends RuntimeException
{
    public function __construct(string $code)
    {
        parent::__construct($code);
    }
}

/**
 * The digest-addressed selection document reference: sha256 over
 * `PHP_SELECTION_DIGEST_DOMAIN || canonical(selection)`. Every command
 * of the plan carries this digest so the approved plan pins its own
 * selection artifacts; a post-approval edit of the selection member
 * cannot validate without changing the plan digest too.
 */
function php_selection_digest(array $selection): string
{
    return php_domain_digest(PHP_SELECTION_DIGEST_DOMAIN, $selection);
}

/**
 * The plan digest: sha256 over `PHP_PLAN_DIGEST_DOMAIN ||
 * canonical(plan without plan_digest)`.
 */
function php_plan_digest(array $plan): string
{
    unset($plan['plan_digest']);
    return php_domain_digest(PHP_PLAN_DIGEST_DOMAIN, $plan);
}

/**
 * Validate and normalize the closed selection manifest the host
 * validated (module roots, explicit edges, checked test bindings,
 * mandatory cross-module gate ids). Unknown members are refused —
 * the manifest is a closed document, never a guessing input.
 *
 * Shape:
 *   {schema_version: "lekalo/native-selection/v0.4.0", completeness: string,
 *    modules: [{id, roots: list<string>, gates: list<string>}],
 *    edges: [{from, to, kind: "dependency"|"target-binding"}],
 *    tests: [{suite_id, module, gate_id}],
 *    mandatory_gate_ids: list<string>}
 */
function php_decode_selection_manifest(array $manifest): array
{
    if (($manifest['schema_version'] ?? null) !== 'lekalo/native-selection/v0.4.0') {
        throw new PhpPlanRefusal('selection-manifest-version');
    }
    $allowed = ['schema_version', 'completeness', 'modules', 'edges', 'tests', 'mandatory_gate_ids'];
    foreach (array_keys($manifest) as $key) {
        if (!in_array($key, $allowed, true)) {
            throw new PhpPlanRefusal('selection-manifest-member');
        }
    }
    $completeness = $manifest['completeness'] ?? 'complete';
    if (!in_array($completeness, ['complete', 'incomplete', 'unknown'], true)) {
        throw new PhpPlanRefusal('selection-manifest-completeness');
    }
    $modules = [];
    $moduleIds = [];
    foreach (($manifest['modules'] ?? []) as $module) {
        if (!is_array($module) || !isset($module['id'], $module['roots'], $module['gates'])
            || !is_string($module['id']) || !preg_match('/^[a-z0-9][a-z0-9._-]{0,63}$/', $module['id'])
            || !is_array($module['roots']) || !is_array($module['gates'])) {
            throw new PhpPlanRefusal('selection-manifest-module');
        }
        if (isset($moduleIds[$module['id']]) || count($modules) >= PHP_MAX_MODULES) {
            throw new PhpPlanRefusal('selection-manifest-module');
        }
        $moduleIds[$module['id']] = true;
        $roots = [];
        foreach ($module['roots'] as $root) {
            if (!is_string($root) || $root === '' || str_starts_with($root, '/')
                || str_contains($root, '\\') || str_contains($root, '..')) {
                throw new PhpPlanRefusal('selection-manifest-root');
            }
            $roots[] = $root;
        }
        $gates = [];
        foreach ($module['gates'] as $gateId) {
            if (!is_string($gateId) || !preg_match('/^[a-z0-9][a-z0-9._-]{0,63}$/', $gateId)) {
                throw new PhpPlanRefusal('selection-manifest-gate');
            }
            $gates[] = $gateId;
        }
        sort($roots);
        sort($gates);
        $modules[] = ['id' => $module['id'], 'roots' => $roots, 'gates' => $gates];
    }
    $edges = [];
    foreach (($manifest['edges'] ?? []) as $edge) {
        if (!is_array($edge) || !isset($edge['from'], $edge['to'], $edge['kind'])
            || !isset($moduleIds[$edge['from']]) || !isset($moduleIds[$edge['to']])
            || $edge['from'] === $edge['to']
            || !in_array($edge['kind'], ['dependency', 'target-binding'], true)) {
            throw new PhpPlanRefusal('selection-manifest-edge');
        }
        $edges[] = ['from' => $edge['from'], 'to' => $edge['to'], 'kind' => $edge['kind']];
    }
    $tests = [];
    $testKeys = [];
    foreach (($manifest['tests'] ?? []) as $test) {
        if (!is_array($test) || !isset($test['suite_id'], $test['module'], $test['gate_id'])
            || !is_string($test['suite_id']) || !preg_match('/^[a-z0-9][a-z0-9._-]{0,63}$/', $test['suite_id'])
            || !isset($moduleIds[$test['module']])
            || !is_string($test['gate_id'])) {
            throw new PhpPlanRefusal('selection-manifest-test');
        }
        // One suite may be reachable through several gates (a filtered
        // leaf and a full-run aggregate); only the exact binding triple
        // must be unique.
        $key = $test['module'] . "\n" . $test['suite_id'] . "\n" . $test['gate_id'];
        if (isset($testKeys[$key])) {
            throw new PhpPlanRefusal('selection-manifest-test');
        }
        $testKeys[$key] = true;
        $tests[] = ['suite_id' => $test['suite_id'], 'module' => $test['module'], 'gate_id' => $test['gate_id']];
    }
    $mandatory = [];
    foreach (($manifest['mandatory_gate_ids'] ?? []) as $gateId) {
        if (!is_string($gateId) || !preg_match('/^[a-z0-9][a-z0-9._-]{0,63}$/', $gateId)) {
            throw new PhpPlanRefusal('selection-manifest-mandatory');
        }
        $mandatory[] = $gateId;
    }
    if (count($mandatory) > PHP_MAX_MANDATORY) {
        throw new PhpPlanRefusal('selection-manifest-mandatory');
    }
    sort($mandatory);
    return [
        'completeness' => $completeness,
        'modules' => $modules,
        'edges' => $edges,
        'tests' => $tests,
        'mandatory_gate_ids' => $mandatory,
    ];
}

/**
 * The deepest module whose declared root is a path prefix of the
 * changed logical path, or null when ownership is unknown (bare
 * symbols always stay unattributed uncertainty).
 *
 * @param list<array{id: string, roots: list<string>, gates: list<string>}> $modules
 */
function php_module_of_path(array $modules, string $path): ?string
{
    $best = null;
    $bestLength = -1;
    foreach ($modules as $module) {
        foreach ($module['roots'] as $root) {
            $prefix = $root === '.' ? '' : rtrim($root, '/');
            $matches = $prefix === ''
                || $path === $prefix
                || str_starts_with($path, $prefix . '/');
            if ($matches && strlen($prefix) > $bestLength) {
                $best = $module['id'];
                $bestLength = strlen($prefix);
            }
        }
    }
    return $best;
}

/**
 * The affected closure: changed modules plus the reverse transitive
 * dependency closure over the declared module edges, with bounded
 * reason provenance. Deterministic sorted traversal.
 *
 * @param list<array{from: string, to: string, kind: string}> $edges
 * @return list<array{package_id: string, reasons: list<array<string, mixed>>}>
 */
function php_affected_closure(array $edges, array $changedModules): array
{
    $consumers = [];
    foreach ($edges as $edge) {
        $consumers[$edge['to']][] = $edge['from'];
    }
    $affected = [];
    $visit = function (string $moduleId, string $kind, string $sourceRef, array $path) use (&$visit, &$affected, $consumers): void {
        if (count($path) > 32) {
            return; // bounded explanation paths
        }
        $reason = ['kind' => $kind, 'source_ref' => $sourceRef, 'edge_path' => $path];
        if (isset($affected[$moduleId])) {
            foreach ($affected[$moduleId]['reasons'] as $existing) {
                if ($existing['kind'] === $kind && $existing['source_ref'] === $sourceRef) {
                    return;
                }
            }
            $affected[$moduleId]['reasons'][] = $reason;
            usort($affected[$moduleId]['reasons'], static fn (array $l, array $r): int
                => strcmp($l['kind'], $r['kind']) ?: strcmp($l['source_ref'], $r['source_ref']));
        } else {
            $affected[$moduleId] = ['package_id' => $moduleId, 'reasons' => [$reason]];
        }
        foreach ($consumers[$moduleId] ?? [] as $consumer) {
            $nextKind = $kind === 'changed-module' ? 'dependent-closure' : $kind;
            $visit($consumer, $nextKind, $kind === 'changed-module' ? $moduleId : $sourceRef, [...$path, $moduleId]);
        }
    };
    $changed = array_unique($changedModules);
    sort($changed);
    foreach ($changed as $moduleId) {
        $visit($moduleId, 'changed-module', $moduleId, []);
    }
    $list = array_values($affected);
    usort($list, static fn (array $l, array $r): int => strcmp($l['package_id'], $r['package_id']));
    return $list;
}

/**
 * Build the proposed native gate plan (issue #61). All inputs are
 * already validated: `$confirmed` from `php_verify_confirmations`,
 * `$manifest` the decoded selection manifest, `$custody` the pinned
 * digests. Missing confirmations exclude a module with a stable
 * reason — they never invent commands; mandatory cross-module gates
 * are always unioned into the selection.
 *
 * Input keys:
 *   composer            array  — the decoded composer.json (name)
 *   composer_lock_state string — present|absent|unreadable
 *   composer_lock_digest ?string
 *   policy              array  — the checked-in execution policy
 *   confirmed           list   — verified confirmation tuples
 *   changes             array  — {files: [{path, change}], symbols: []}
 *   selection_manifest  array  — the host-validated selection document
 *   custody             array  — input_manifest_digest, tool_catalog_digest,
 *                                capability_snapshot_digest, scan_ref, observed_ref,
 *                                profile_id, profile_digest, adapter_identity
 */
function php_build_native_plan(array $input): array
{
    foreach (['composer', 'policy', 'confirmed', 'changes', 'selection_manifest', 'custody'] as $key) {
        if (!array_key_exists($key, $input)) {
            throw new PhpPlanRefusal('plan-input-missing-' . $key);
        }
    }
    $composer = $input['composer'];
    $policy = $input['policy'];
    $confirmed = $input['confirmed'];
    $changes = $input['changes'];
    $custody = $input['custody'];
    $manifest = php_decode_selection_manifest($input['selection_manifest']);
    $packageName = $composer['name'] ?? null;
    if (!is_string($packageName) || $packageName === '') {
        throw new PhpPlanRefusal('composer-manifest-invalid');
    }
    $packageId = '.' . '=' . $packageName;

    // Attribute the changed inputs onto modules; unattributable
    // entries are recorded uncertainty, never a silent empty green.
    $uncertainties = [];
    $changedModules = [];
    $files = is_array($changes['files'] ?? null) ? $changes['files'] : [];
    if (count($files) > PHP_MAX_CHANGED_FILES) {
        throw new PhpPlanRefusal('changes-bound');
    }
    foreach ($files as $file) {
        $path = is_array($file) ? ($file['path'] ?? null) : null;
        if (!is_string($path)) {
            throw new PhpPlanRefusal('changes-entry');
        }
        $moduleId = php_module_of_path($manifest['modules'], $path);
        if ($moduleId === null) {
            $uncertainties[] = [
                'kind' => 'unknown',
                'detail' => 'changed path without module attribution: ' . substr($path, 0, 128),
            ];
            continue;
        }
        $changedModules[] = $moduleId;
    }
    foreach ((is_array($changes['symbols'] ?? null) ? $changes['symbols'] : []) as $symbol) {
        if (!is_string($symbol)) {
            throw new PhpPlanRefusal('changes-entry');
        }
        $uncertainties[] = [
            'kind' => 'unknown',
            'detail' => 'changed symbol without module attribution: ' . substr($symbol, 0, 128),
        ];
    }
    $affected = php_affected_closure($manifest['edges'], $changedModules);
    $affectedIds = array_map(static fn (array $entry): string => $entry['package_id'], $affected);
    // The plan workspace has exactly one package (the Composer root):
    // affected entries stay package-scoped, and the module provenance
    // travels in the closed reason kinds (source_ref names the module).
    $packageAffectedReasons = [];
    $seenReasons = [];
    foreach ($affected as $entry) {
        foreach ($entry['reasons'] as $reason) {
            $kind = $reason['kind'] === 'changed-module' ? 'changed-package' : $reason['kind'];
            $key = $kind . "\n" . $reason['source_ref'];
            if (isset($seenReasons[$key])) {
                continue;
            }
            $seenReasons[$key] = true;
            $packageAffectedReasons[] = [
                'kind' => $kind,
                'source_ref' => substr($reason['source_ref'], 0, 512),
                'edge_path' => array_slice($reason['edge_path'], 0, 32),
            ];
        }
    }
    $packageAffected = [];
    if ($packageAffectedReasons !== []) {
        // Package-scoped reasons carry module provenance in source_ref;
        // the edge_path member stays absent (its hops are package ids,
        // and the single-root workspace has no package hops to name).
        $packageAffected[] = ['package_id' => $packageId, 'reasons' => array_map(
            static function (array $reason): array {
                unset($reason['edge_path']);
                return $reason;
            },
            $packageAffectedReasons,
        )];
    }

    // Gate selection: affected modules' bound gates unioned with the
    // mandatory cross-module gates, joined per stable gate id, exactly
    // once. An aggregate gate that covers a suite suppresses the leaf
    // gates covering the same suites (exactly-once execution).
    $confirmedByGate = [];
    foreach ($confirmed as $tuple) {
        if ($tuple['package_id'] !== $packageId) {
            continue;
        }
        $confirmedByGate[$tuple['gate_id']] = $tuple;
    }
    $selectedGateIds = [];
    $excluded = [];
    $moduleGaps = [];
    $selectionMode = 'targeted';
    $fallbackRuleRef = null;
    foreach ($affectedIds as $moduleId) {
        $module = null;
        foreach ($manifest['modules'] as $candidate) {
            if ($candidate['id'] === $moduleId) {
                $module = $candidate;
                break;
            }
        }
        if ($module === null) {
            $moduleGaps[] = 'affected module without declared gates: ' . $moduleId;
            continue;
        }
        if ($module['gates'] === []) {
            $moduleGaps[] = 'affected module without confirmed gate: ' . $moduleId;
            continue;
        }
        foreach ($module['gates'] as $gateId) {
            if (!isset($confirmedByGate[$gateId])) {
                $moduleGaps[] = 'gate without confirmed recipe: ' . $gateId;
                continue;
            }
            $selectedGateIds[$gateId] = true;
        }
    }
    foreach ($moduleGaps as $gap) {
        $uncertainties[] = ['kind' => 'unknown', 'detail' => substr($gap, 0, 256), 'package_id' => $packageId];
    }
    // A mandatory gate without a confirmed recipe is a blocker, never
    // a silent omission.
    foreach ($manifest['mandatory_gate_ids'] as $gateId) {
        if (!isset($confirmedByGate[$gateId])) {
            throw new PhpPlanRefusal('mandatory-gate-unconfirmed-' . substr($gateId, 0, 48));
        }
    }
    // Mandatory cross-module gates ride every targeted selection that
    // executes anything at all (an empty selection claims no coverage,
    // so it cannot name unexecuted mandatory gates either).
    if ($selectedGateIds !== [] && $selectionMode === 'targeted') {
        foreach ($manifest['mandatory_gate_ids'] as $gateId) {
            $selectedGateIds[$gateId] = true;
        }
    }
    // Explicit full fallback: only a checked release-full rule with a
    // recorded digest expands the selection to the full confirmed
    // inventory, and the expansion enumerates every gate — never a
    // label over the affected list. The expansion rides before the
    // exactly-once suppression, so even a full run keeps one owner per
    // suite (issue acceptance: no double-run).
    $fallbackRule = $policy['fallback_rule'] ?? ['mode' => 'none'];
    if (is_array($fallbackRule) && ($fallbackRule['mode'] ?? null) === 'release-full') {
        $ruleDigest = $fallbackRule['rule_digest'] ?? null;
        if (!is_string($ruleDigest) || !preg_match('/^sha256:[0-9a-f]{64}$/', $ruleDigest)) {
            throw new PhpPlanRefusal('fallback-rule-digest');
        }
        $selectionMode = 'release-full';
        $fallbackRuleRef = $ruleDigest;
        foreach ($confirmedByGate as $gateId => $tuple) {
            $selectedGateIds[$gateId] = true;
        }
    }
    ksort($selectedGateIds);
    // Exactly-once suites (after the full expansion): each bound suite
    // is owned by the covering gate with the widest coverage (the
    // aggregate), ties broken by the smallest gate id; leaf gates
    // bound to a suite already owned by another selected gate are
    // suppressed — the suite executes exactly once (no double-run).
    $coveringBySuite = [];
    foreach (array_keys($selectedGateIds) as $gateId) {
        foreach ($confirmedByGate[$gateId]['covers_suite_ids'] as $suiteId) {
            $coveringBySuite[$suiteId][] = $gateId;
        }
    }
    $coveredSuites = [];
    foreach ($coveringBySuite as $suiteId => $gateIds) {
        $gateIds = array_values(array_unique($gateIds));
        usort($gateIds, static function (string $left, string $right) use ($confirmedByGate): int {
            $leftCount = count($confirmedByGate[$left]['covers_suite_ids']);
            $rightCount = count($confirmedByGate[$right]['covers_suite_ids']);
            return $rightCount <=> $leftCount ?: strcmp($left, $right);
        });
        $coveredSuites[$suiteId] = $gateIds[0];
    }
    $suiteOwnerByGate = [];
    foreach ($manifest['tests'] as $test) {
        $suiteOwnerByGate[$test['gate_id']][] = $test['suite_id'];
    }
    foreach (array_keys($selectedGateIds) as $gateId) {
        foreach ($suiteOwnerByGate[$gateId] ?? [] as $suiteId) {
            $owner = $coveredSuites[$suiteId] ?? null;
            if ($owner !== null && $owner !== $gateId) {
                // The suite already rides the covering aggregate gate.
                unset($selectedGateIds[$gateId]);
                break;
            }
        }
    }

    ksort($selectedGateIds);
    // An empty selection is an honest blocked plan, never a refusal:
    // nothing is affected, nothing executes, and the plan claims no
    // coverage (the mandatory gate ids ride only selections that run).
    $emptySelection = $selectedGateIds === [];
    if ($emptySelection) {
        $excluded = [['package_id' => $packageId, 'reason' => 'no-reason']];
    }

    // Commands: deterministic order by gate id; every command carries
    // the pinned selection reference and its covered suites.
    $selection = [
        'mode' => $selectionMode,
        'modules' => array_values(array_unique(array_merge(
            $affectedIds,
            array_map(static function (string $gateId) use ($manifest, $packageId): string {
                // Modules of mandatory gates ride the selection too.
                foreach ($manifest['modules'] as $module) {
                    if (in_array($gateId, $module['gates'], true)) {
                        return $module['id'];
                    }
                }
                return 'root';
            }, array_keys($selectedGateIds)),
        ))),
        'tests' => [],
        'mandatory_gate_ids' => $emptySelection ? [] : $manifest['mandatory_gate_ids'],
        'excluded' => $excluded,
        'uncertainties' => $uncertainties,
        'fallback_rule_ref' => $fallbackRuleRef,
    ];
    sort($selection['modules']);
    $suiteIds = [];
    foreach ($manifest['tests'] as $test) {
        if (isset($selectedGateIds[$test['gate_id']])) {
            $suiteIds[$test['suite_id']] = true;
        }
    }
    $selection['tests'] = array_keys($suiteIds);
    sort($selection['tests']);
    $selectionRef = php_selection_digest($selection);

    // Commands: deterministic order by gate id; every command carries
    // the pinned selection reference and its covered suites. The
    // reason provenance distinguishes affected-module gates, mandatory
    // cross-module gates, and full-fallback expansion.
    $gateModule = [];
    foreach ($manifest['modules'] as $module) {
        foreach ($module['gates'] as $gateId) {
            $gateModule[$gateId] = $module['id'];
        }
    }
    $commands = [];
    foreach (array_keys($selectedGateIds) as $gateId) {
        $tuple = $confirmedByGate[$gateId];
        $reasonRef = $packageId . '#explicit-binding';
        foreach ($affected as $entry) {
            if (($gateModule[$gateId] ?? null) === $entry['package_id']) {
                $reasonRef = $packageId . '#' . $entry['reasons'][0]['kind'];
                break;
            }
        }
        if (!isset($gateModule[$gateId]) && $fallbackRuleRef !== null) {
            $reasonRef = $packageId . '#release-rule';
        }
        $commands[] = [
            'id' => 'gate-' . $gateId,
            'package_id' => $packageId,
            'gate' => $tuple['gate'],
            'gate_id' => $gateId,
            'gate_kind' => $tuple['gate_kind'],
            'required' => $tuple['required'],
            'selection_ref' => $selectionRef,
            'covers_suite_ids' => $tuple['covers_suite_ids'],
            'script_name' => $tuple['script_name'],
            'script_digest' => $tuple['script_digest'],
            'confirmation_ref' => $tuple['confirmation_ref'],
            'cwd' => $tuple['cwd'],
            'tool_ref' => $tuple['tool_ref'],
            'argv' => $tuple['argv'],
            'env' => [],
            'depends_on' => [],
            'affected_reason_refs' => [$reasonRef],
            'read_manifest_ref' => (string) $custody['input_manifest_digest'],
            'allowed_writes' => ['mode' => 'stage-only'],
            'limits' => $policy['limits'],
        ];
    }
    // Prerequisite ordering rides the declared module edges: an edge
    // is consumer -> dependency (the plan workspace convention), so a
    // command of a consuming module depends on every selected command
    // of its dependency modules (bounded transitive chain).
    $commandsByModule = [];
    foreach ($commands as $index => $command) {
        $commandsByModule[($gateModule[$command['gate_id']] ?? 'root')][] = $index;
    }
    foreach ($commands as $index => $command) {
        $moduleId = $gateModule[$command['gate_id']] ?? null;
        if ($moduleId === null) {
            continue;
        }
        $prerequisites = [];
        $queue = [$moduleId];
        $visited = [];
        while ($queue !== []) {
            $current = array_shift($queue);
            if (isset($visited[$current])) {
                continue;
            }
            $visited[$current] = true;
            foreach ($manifest['edges'] as $edge) {
                if ($edge['from'] === $current && !isset($visited[$edge['to']])) {
                    // The dependency side of the edge.
                    $queue[] = $edge['to'];
                    foreach ($commandsByModule[$edge['to']] ?? [] as $prerequisiteIndex) {
                        $prerequisites[] = $commands[$prerequisiteIndex]['id'];
                    }
                }
            }
        }
        $prerequisites = array_values(array_unique(array_diff($prerequisites, [$command['id']])));
        sort($prerequisites);
        if ($prerequisites !== []) {
            $commands[$index]['depends_on'] = array_slice($prerequisites, 0, 128);
        }
    }

    $toolCatalog = $input['tool_catalog'] ?? [];
    $plan = [
        'schema_version' => PHP_PLAN_SCHEMA_VERSION,
        'kind' => 'native-plan',
        'plan_digest' => 'sha256:' . str_repeat('0', 64),
        'adapter' => $custody['adapter_identity'],
        'planner_version' => PHP_PLANNER_VERSION,
        'canonicalization_version' => PHP_CANONICALIZATION_VERSION,
        'repository_role' => $policy['repository_role'],
        'trust' => $policy['trust'],
        'authority_ref' => $policy['authority_ref'],
        'policy_ref' => $policy['policy_ref'],
        'classification_ref' => $policy['classification_ref'],
        'execution_policy_ref' => [
            'id' => $policy['identity']['id'],
            'version' => $policy['identity']['version'],
            'digest' => $policy['policy_digest'],
        ],
        'profile_ref' => [
            'id' => $custody['profile_id'],
            'digest' => $custody['profile_digest'],
        ],
        'input_manifest_digest' => (string) $custody['input_manifest_digest'],
        'scan_ref' => [
            'id' => 'scan',
            'version' => PHP_PLANNER_VERSION,
            'digest' => (string) $custody['scan_ref'],
        ],
        'observed_ref' => [
            'id' => 'observed',
            'version' => PHP_PLANNER_VERSION,
            'digest' => (string) $custody['observed_ref'],
        ],
        'tool_catalog_digest' => (string) $custody['tool_catalog_digest'],
        'capability_snapshot_digest' => (string) $custody['capability_snapshot_digest'],
        'workspace' => [
            'manager' => 'composer-project',
            'declared_version' => 'unknown',
            'compatibility_path' => 'partial',
            'root' => '.',
            'manifest_digest' => $confirmed[0]['manifest_digest'],
            'lock_digest_state' => $input['composer_lock_state'] ?? 'absent',
            'lock_digest' => $input['composer_lock_digest'] ?? null,
            'packages' => [[
                'id' => $packageId,
                'name' => $packageName,
                'root' => '.',
                'manifest_digest' => $confirmed[0]['manifest_digest'],
            ]],
            'edges' => [],
            'completeness' => $manifest['completeness'] === 'complete' && $uncertainties === [] ? 'complete' : 'incomplete',
            'uncertainties' => $uncertainties,
        ],
        'changes' => [
            'files' => array_map(static fn (array $file): array => [
                'path' => (string) $file['path'],
                'change' => in_array($file['change'] ?? null, ['added', 'modified', 'deleted', 'renamed'], true)
                    ? $file['change'] : 'modified',
            ], $files),
            'symbols' => array_values(array_filter(
                is_array($changes['symbols'] ?? null) ? $changes['symbols'] : [],
                'is_string',
            )),
        ],
        'affected' => $packageAffected,
        'excluded' => $excluded,
        'selection_mode' => $selectionMode,
        'selection' => $selection,
        'commands' => $commands,
        'env' => $policy['env_recipe'],
        'tools' => $toolCatalog,
        'required_capabilities' => [PHP_PLAN_CAPABILITY],
        'capabilities' => [[
            'id' => PHP_PLAN_CAPABILITY,
            'definition_version' => PHP_PLANNER_VERSION,
            'state' => 'full',
            'source' => 'declared',
        ]],
        'run_eligibility' => [
            'state' => $emptySelection ? 'blocked' : 'plan-only',
            'reason_codes' => $emptySelection
                ? ['no-commands', 'fixture-runner-not-in-production']
                : ['fixture-runner-not-in-production'],
        ],
        'limits' => $policy['limits'],
        'write_policy' => $policy['write_policy'],
    ];
    $plan['plan_digest'] = php_plan_digest($plan);
    return $plan;
}

exit(main());
