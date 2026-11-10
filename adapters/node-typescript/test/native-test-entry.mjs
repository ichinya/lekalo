/** Source/process authoring harness; not part of any deployed runtime bundle.
 * The core sandbox and public CLI gates exercise production peer supervision. */
import * as adapter from '../adapter.mjs';
import {configureTestCompiler} from './compiler-test-runtime.mjs';
configureTestCompiler(adapter);
try { await adapter.__lekaloKernel.main(); }
finally { adapter.compilerHostApi.close(); }
