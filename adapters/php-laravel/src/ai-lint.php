<?php
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
    // Bound the document-level span union without shortening any record or
    // activation chain. Canonical record order determines the retained slice.
    $locationLimited=count($locations)>32;$limit=$locationLimited?['document-location-limit']:[];
    if ($locationLimited) {
        $retained=[];$kept=[];
        foreach ($records as $id=>$record) {
            $refs=array_fill_keys($record['locations'],true);
            foreach ($record['activation'] as $step) {foreach ($step['locations'] as $location) {$refs[$location]=true;}}
            if (count($retained)+count(array_diff_key($refs,$retained))>32) {continue;}
            $retained+=$refs;$kept[$id]=$record;
        }
        foreach ($locations as $id=>$location) {if (count($retained)<32) {$retained[$id]=true;}}
        $locations=array_intersect_key($locations,$retained);$records=$kept;
    }
    $rules=['ambiguity.implicit-target-defaults','ambiguity.multiple-resolutions','ambiguity.scattered-state-writes','hidden.convention-only-path','hidden.dispatch-without-binding','hidden.observer-write','hidden.path-without-trace-owner','hidden.reflective-call','hidden.string-reference','hidden.undeclared-effect','indirection.depth-exceeded'];$coverage=[];
    foreach ($rules as $rule) {foreach ($lint['scope'] as $scope) {$supported=in_array($rule,['hidden.observer-write','hidden.reflective-call','ambiguity.scattered-state-writes'],true);$coverage[]=['rule'=>$rule,'target'=>$request['target'],'scope'=>$scope,'state'=>$supported&&count($files)>0?'partial':'unsupported','eligible'=>lint_unknown(),'examined'=>lint_known(count($methods)),'limitations'=>$supported?['bounded-token-files',...$limit]:['detector-unsupported']];}}
    return ['schemaVersion'=>'lekalo/ai-lint-evidence/v0.6.4','identity'=>'dev.lekalo.ai-lint-evidence@0.6.4','target'=>$request['target'],'scope'=>$lint['scope'],'producer'=>['id'=>ADAPTER_ID,'version'=>ADAPTER_VERSION,'artifactDigest'=>'sha256:'.hash_file('sha256', __FILE__),'tool'=>'php-token-static','compiler'=>lint_known('php/'.PHP_MAJOR_VERSION.'.'.PHP_MINOR_VERSION),'framework'=>lint_unknown(),'recipe'=>'ai-readability/1'],'pins'=>$lint['pins'],'inputManifestDigest'=>lint_hash($sources),'sources'=>$sources,'locations'=>array_values($locations),'records'=>array_values($records),'coverage'=>$coverage,'limitations'=>['static-files-only','no-application-execution','framework-version-unknown','external-class-resolution-unsupported',...$limit]];
}
