#!/usr/bin/env python3
"""Exercise the locally built hardened container with temporary keys and storage.

Prerequisites: docker build -t trust-service:local . ; Docker and OpenSSL.
All financial operations below are local ledger writes. No broker is contacted.
Only the uniquely named container and volume created here are cleaned up.
"""
import base64, json, os, pathlib, subprocess, tempfile, time, urllib.request, urllib.error, uuid
from reliability import inventory

def run(*args, **kwargs):
    return subprocess.run(args, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kwargs).stdout

def b64(data):
    return base64.urlsafe_b64encode(data).rstrip(b'=').decode()

name='trust-smoke-'+uuid.uuid4().hex[:12]
volume=name+'-data'
def container_logs():
    return subprocess.run(['docker','logs',name],check=False,stdout=subprocess.PIPE,stderr=subprocess.STDOUT).stdout

with tempfile.TemporaryDirectory(prefix='trust-container-smoke-') as folder:
    root=pathlib.Path(folder)
    private=root/'key.pem'
    run('openssl','genpkey','-algorithm','RSA','-pkeyopt','rsa_keygen_bits:2048','-out',str(private))
    modulus=run('openssl','rsa','-in',str(private),'-noout','-modulus').decode().strip().split('=',1)[1]
    (root/'jwks.json').write_text(json.dumps({'keys':[{'kty':'RSA','use':'sig','alg':'RS256','kid':'smoke','n':b64(bytes.fromhex(modulus)),'e':'AQAB'}]}))
    scopes=['trust:read','trust:preview','trust:trade','trust:reconcile','trust:manual','trust:journal','trust:policy','trust:admin']
    config={'listen':'0.0.0.0:8000','public_url':'https://trust.test','issuer':'https://issuer.test','jwks_file':'/etc/trust/jwks.json','database':'/data/financial.db','journal':'/data/control.db','reconcile_interval_seconds':0,'grants':{'smoke-owner':{'all_accounts':True,'scopes':scopes}}}
    (root/'config.json').write_text(json.dumps(config))
    for file in [root/'config.json',root/'jwks.json']: file.chmod(0o444)
    now=int(time.time())
    unsigned=b64(json.dumps({'alg':'RS256','kid':'smoke','typ':'at+jwt'}).encode())+'.'+b64(json.dumps({'iss':config['issuer'],'aud':config['public_url']+'/mcp','sub':'smoke-owner','iat':now,'exp':now+900,'scope':' '.join(scopes)}).encode())
    token=unsigned+'.'+b64(run('openssl','dgst','-sha256','-sign',str(private),input=unsigned.encode()))
    try:
        run('docker','run','-d','--name',name,'--read-only','--cap-drop','ALL','--security-opt','no-new-privileges:true','--user','10001:10001','--pids-limit','128','--memory','512m','--tmpfs','/tmp:rw,noexec,nosuid,size=16m','-p','127.0.0.1::8000','-v',volume+':/data','-v',str(root/'config.json')+':/etc/trust/config.json:ro','-v',str(root/'jwks.json')+':/etc/trust/jwks.json:ro','trust-service:local')
        address=run('docker','port',name,'8000/tcp').decode().strip()
        def request(path, payload=None, authorized=True, mcp=False):
            headers={'Host':'trust.test','Content-Type':'application/json'}
            if authorized: headers['Authorization']='Bearer '+token
            if mcp: headers.update({'Accept':'application/json, text/event-stream','MCP-Protocol-Version':'2025-11-25'})
            req=urllib.request.Request('http://'+address+path,headers=headers,data=None if payload is None else json.dumps(payload).encode())
            try:
                with urllib.request.urlopen(req,timeout=10) as response:
                    data=response.read()
                    return response.status,json.loads(data) if 'application/json' in response.headers.get('Content-Type','') else data.decode()
            except urllib.error.HTTPError as e: return e.code,json.loads(e.read())
        for _ in range(60):
            try:
                if request('/readyz',authorized=False)[0]==200: break
            except (OSError,urllib.error.URLError): pass
            time.sleep(.2)
        else: raise AssertionError('container readiness failed')
        assert run('docker','exec',name,'id','-u').strip()==b'10001'
        assert subprocess.run(['docker','exec',name,'touch','/must-not-write'],stdout=subprocess.PIPE,stderr=subprocess.PIPE).returncode!=0
        assert request('/v1/tools',authorized=False)[0]==401
        status,tools=request('/v1/tools');assert status==200
        assert {tool['name'] for tool in tools['tools']}==set(inventory()['tools'])
        def call(tool,args):
            status,result=request('/v1/tools/'+tool,args);assert status==200 and result['error'] is None,(tool,result);return result
        account=call('account_create',{'name':'container-paper','description':'smoke test','environment':'paper','broker':'ibkr','broker_account_id':'DU_CONTAINER_TEST','idempotency_key':'container-create'})['data']['id']
        deposit={'account_id':account,'amount':'10.25','currency':'usd','idempotency_key':'container-deposit'}
        first=call('capital_deposit',deposit)
        policy=call('risk_rule_create',{'account_id':account,'kind':'per_trade','percentage':'0.3','priority':'error','description':'fractional smoke policy','idempotency_key':'container-policy'})
        assert str(policy['data']['name']['RiskPerTrade'])=='0.3'
        assert call('capital_deposit',deposit)['operation_id']==first['operation_id']
        status,result=request('/mcp',{'jsonrpc':'2.0','id':1,'method':'tools/call','params':{'name':'service_status','arguments':{}}},mcp=True)
        assert status==200 and result['result']['structuredContent']['data']['service']=='trust'
        assert request('/metrics',authorized=False)[0]==401
        status,metrics=request('/metrics');assert status==200 and 'trust_worker_ready 1\n' in metrics and 'trust_execution_milliseconds_bucket' in metrics
        events=call('audit_list',{})['data']['events']
        assert any(event['event']['version']==2 and event['event']['outcome']=='replayed' and len(event['event_hash'])==64 for event in events)
        run('docker','restart','--time','10',name)
        address=run('docker','port',name,'8000/tcp').decode().strip()
        for _ in range(60):
            try:
                if request('/readyz',authorized=False)[0]==200: break
            except (OSError,urllib.error.URLError): pass
            time.sleep(.2)
        assert call('capital_deposit',deposit)['operation_id']==first['operation_id']
        assert call('transactions_list',{'account_id':account})['data']['total']==1
        policies=call('risk_rules_list',{'account_id':account})['data']['items']
        assert len(policies)==1 and str(policies[0]['name']['RiskPerTrade'])=='0.3'
        assert request('/v1/tools/audit_verify',{})[1]['error'] is None
        assert token.encode() not in container_logs()
        print('PASS: nonroot/read-only container, JWT auth denial, full catalog, fractional risk policy, MCP call, durable replay across restart, audit v2 verification, authenticated metrics, token-free logs')
    except Exception:
        logs=container_logs().decode().replace(token,'[REDACTED]')
        pathlib.Path('/tmp/trust-smoke-service.log').write_text(logs)
        raise
    finally:
        subprocess.run(['docker','stop','--time','10',name],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        subprocess.run(['docker','rm',name],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        subprocess.run(['docker','volume','rm',volume],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
