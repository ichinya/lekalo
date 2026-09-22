# THIRD_PARTY_NOTICES

This adapter artifact (`adapter.mjs`) embeds third-party software. The
embedded code and data are unmodified releases of the pinned versions
listed below, combined by the repository's deterministic build script
(`adapters/node-typescript/build.mjs`). The project's own code is MIT OR
Apache-2.0; the notices below apply to the embedded third-party parts.

## TypeScript 5.9.3

- Source: <https://github.com/microsoft/TypeScript/tree/v5.9.3>
- npm package: `typescript@5.9.3`
- License: Apache-2.0, Copyright (c) Microsoft Corporation. All rights
  reserved.
- Embedded content: the compiled compiler API (`lib/typescript.js`) and
  the standard-library declaration files (`lib/lib.*.d.ts`) of exactly
  this version, embedded as data by `build.mjs`.

```
                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
```

## esbuild 0.25.12 (build-time only, never shipped as code)

- Source: <https://github.com/evanw/esbuild/tree/v0.25.12>
- npm package: `esbuild@0.25.12`
- License: MIT, Copyright (c) 2020 Evan Wallace
- Role: the bundler that produced the committed artifact. esbuild code
  is not executed at adapter runtime and none of its code is embedded in
  `adapter.mjs`; it is listed here because it is part of the artifact's
  build provenance.

```
MIT License

Copyright (c) 2020 Evan Wallace

Permission is hereby granted, free of charge, to any person obtaining a
copy of this software and associated documentation files (the
"Software"), to deal in the Software without restriction, including
without limitation the rights to use, copy, modify, merge, publish,
distribute, sublicense, and/or sell copies of the Software, and to
permit persons to whom the Software is furnished to do so, subject to
the following conditions:

The above copyright notice and this permission notice shall be included
in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```

## zod 3.25.76 (test-time only, never shipped as code)

- Source: <https://github.com/colinhacks/zod/tree/v3.25.76>
- npm package: `zod@3.25.76`
- License: MIT, Copyright (c) 2020 Colin McDonnell
- Role: the schema runtime the generated Zod modules import. The
  adapter never embeds or bundles zod; the dev dependency exists only so
  the committed fixture suites can typecheck and runtime-execute the
  generated output. Consumers of the generated schemas supply their own
  zod (minimum 3.22).

```
MIT License

Copyright (c) 2020 Colin McDonnell

Permission is hereby granted, free of charge, to any person obtaining a
copy of this software and associated documentation files (the
"Software"), to deal in the Software without restriction, including
without limitation the rights to use, copy, modify, merge, publish,
distribute, sublicense, and/or sell copies of the Software, and to
permit persons to whom the Software is furnished to do so, subject to
the following conditions:

The above copyright notice and this permission notice shall be included
in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```

## Pinning and custody

The exact versions above are enforced by
`adapters/node-typescript/package.json` and its `package-lock.json`.
`build.mjs` refuses to run against any other versions, refuses a missing
platform binary, and refuses any drift between the committed
`adapter.mjs` and a deterministic rebuild (`node build.mjs --check`).
The artifact header comment repeats the version pins and this licensing
summary so the single-file distribution keeps its attribution.
