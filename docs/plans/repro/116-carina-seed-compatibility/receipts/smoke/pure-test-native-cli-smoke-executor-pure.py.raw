import importlib.util,pathlib,os
pad=pathlib.Path(os.environ['LOCALAPPDATA'])/'Temp/opencode/ses_mvkc0sjaqomgtdbffv/task116/preflight'
executor=pad/'execute-bounded-native-cli-smoke.py'
spec=importlib.util.spec_from_file_location('native_cli_smoke_executor_pure_test',executor)
module=importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
assert module.__name__!='__main__'
text=b'usage: novac check <file.nv>... | novac emit <file.nv>'
expected={'stdout_variants':[text.decode()+'\n',text.decode()+'\r\n'],'stderr':'','status':2}
assert module.matches(text+b'\n',b'',2,expected)
assert module.matches(text+b'\r\n',b'',2,expected)
assert not module.matches(text+b'\nextra\n',b'',2,expected)
assert not module.matches(text,b'',2,expected)
assert not module.matches(text+b'\n',b'',0,expected)
hello={'stdout_variants':[''],'stderr':'','status':0}
assert module.matches(b'',b'',0,hello)
assert not module.matches(b'\n',b'',0,hello)
not_attempted=module.execution_evidence(0,[])
assert not_attempted=={'native_exe_executed':False,'confirmed_process_starts':0,'confirmed_not_started':0,'unconfirmed_attempts':0,'state':'not-attempted'}
started=module.execution_evidence(1,[{'spawn_error':None,'exit_status':0}])
assert started['native_exe_executed'] is True and started['confirmed_process_starts']==1 and started['state']=='confirmed-start'
spawn_failed=module.execution_evidence(1,[{'spawn_error':'FileNotFoundError'}])
assert spawn_failed['native_exe_executed'] is False and spawn_failed['confirmed_not_started']==1 and spawn_failed['state']=='confirmed-no-start'
unconfirmed=module.execution_evidence(1,[None])
assert unconfirmed['native_exe_executed'] is None and unconfirmed['unconfirmed_attempts']==1 and unconfirmed['state']=='unknown'
missing_field=module.execution_evidence(1,[{}])
assert missing_field['native_exe_executed'] is None and missing_field['state']=='unknown'
mixed=module.execution_evidence(3,[None,{'spawn_error':None,'exit_status':2},{'spawn_error':'OSError'}])
assert mixed['native_exe_executed'] is True and mixed['confirmed_process_starts']==1 and mixed['confirmed_not_started']==1 and mixed['unconfirmed_attempts']==1
print('pure smoke executor tests: PASS; LF/CRLF green, extra/no-EOL/wrong-status red; empty hello strict; summary false only no attempt/confirmed no-start, true on spawn_error=None, unknown on unconfirmed attempt')
