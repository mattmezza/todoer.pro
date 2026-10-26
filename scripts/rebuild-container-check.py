#!/usr/bin/env python3
"""Exercise a local production image and split Compose with synthetic data only.
Requires Docker Compose, an already-built image and local Moto S3 on127.0.0.1:8937.
Creates/removes only randomly named test containers, networks and volume.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import select
import socket
import socketserver
import sqlite3
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.request
import uuid

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--image',default='todoer-rebuild:local-verified')
args=parser.parse_args()
root=Path(__file__).resolve().parent.parent
project='todoer-smoke-'+uuid.uuid4().hex[:12]
proxy=project+'-proxy'
volume=project+'-data'
public='https://container-smoke.example.invalid'
token='synthetic-container-'+uuid.uuid4().hex

def docker(*arguments,check=True):
    result = subprocess.run(['docker',*arguments],check=False,capture_output=True,text=True)
    if check and result.returncode: raise RuntimeError(result.stderr[-1500:])
    return result

class Relay(socketserver.BaseRequestHandler):
    def handle(self):
        with socket.create_connection(('127.0.0.1',8937),timeout=10) as upstream:
            peers=[self.request,upstream]
            while True:
                ready,_,_=select.select(peers,[],[],30)
                if not ready:return
                for source in ready:
                    data=source.recv(65536)
                    if not data:return
                    (upstream if source is self.request else self.request).sendall(data)
class RelayServer(socketserver.ThreadingTCPServer):
    daemon_threads=True
    allow_reuse_address=True

docker('image','inspect',args.image)
with socket.socket() as probe:
    probe.bind(('127.0.0.1',0));port=probe.getsockname()[1]
Path(root/'.local/rebuild').mkdir(parents=True,exist_ok=True)
with tempfile.TemporaryDirectory(prefix='container-',dir=root/'.local/rebuild') as temporary:
    temporary=Path(temporary)
    database=temporary/'synthetic.db'
    with sqlite3.connect(database) as db:
        db.execute('PRAGMA foreign_keys=ON')
        db.execute('CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP)')
        for migration in sorted((root/'rebuild/migrations').glob('*.sql')):
            db.executescript(migration.read_text())
            db.execute('INSERT INTO schema_migrations(version) VALUES(?)',(int(migration.name[:3]),))
        db.execute("INSERT INTO users(id,name) VALUES('smoke-user','Synthetic container user')")
        db.execute("INSERT INTO lists(id,title) VALUES('smoke-list','Container checklist')")
        db.execute("INSERT INTO memberships(list_id,user_id) VALUES('smoke-list','smoke-user')")
        db.execute("INSERT INTO auth_agents(id,user_id,token_hash,audience,permissions,expires_at) VALUES('smoke-token','smoke-user',?,?,?,?)",(hashlib.sha256(token.encode()).hexdigest(),public+'/api/v1','["read","write"]',int(time.time())+3600))
        db.commit()
    database.chmod(0o600)
    app_env=temporary/'app.env'
    app_env.write_text('TODOER_GOOGLE_CLIENT_ID=synthetic-client\nTODOER_GOOGLE_CLIENT_SECRET=synthetic-not-a-google-secret\nTODOER_SERVER_KEY='+('42'*32)+'\n')
    app_env.chmod(0o600)
    replica_env=temporary/'replica.env'
    bucket='container-recovery-'+uuid.uuid4().hex
    urllib.request.urlopen(urllib.request.Request('http://127.0.0.1:8937/'+bucket,method='PUT')).close()
    def replica(endpoint):
        replica_env.write_text(f'TODOER_REPLICA_ENDPOINT={endpoint}\nTODOER_REPLICA_REGION=us-east-1\nTODOER_REPLICA_BUCKET={bucket}\nTODOER_REPLICA_PREFIX=database\nTODOER_REPLICA_ACCESS_KEY_ID=development\nTODOER_REPLICA_SECRET_ACCESS_KEY=development\n')
        replica_env.chmod(0o600)
    replica('http://127.0.0.1:8937')
    overlay=temporary/'test-compose.json'
    overlay.write_text(json.dumps({'services':{'app':{'ports':[f'127.0.0.1:{port}:8092']}}}))
    env=os.environ.copy()
    env.update(TODOER_REBUILD_IMAGE=args.image,TODOER_REBUILD_APP_ENV=str(app_env),TODOER_REBUILD_REPLICA_ENV=str(replica_env),TODOER_REBUILD_PROXY_NETWORK=proxy,TODOER_REBUILD_VOLUME=volume,TODOER_PUBLIC_URL=public)
    compose=['docker','compose','-p',project,'-f',str(root/'deploy/rebuild/compose.yml'),'-f',str(overlay)]
    def run(*arguments,check=True):return subprocess.run([*compose,*arguments],env=env,check=check,capture_output=True,text=True)
    relay=None
    docker('network','create',proxy)
    try:
        run('create')
        gateway=json.loads(docker('network','inspect',project+'_replication').stdout)[0]['IPAM']['Config'][0]['Gateway']
        relay=RelayServer((gateway,0),Relay)
        threading.Thread(target=relay.serve_forever,daemon=True).start()
        replica(f'http://{gateway}:{relay.server_address[1]}')
        docker('run','--rm','--user','0:0','--entrypoint','sh','-v',volume+':/data','-v',str(database)+':/seed.db:ro',args.image,'-c','cp /seed.db /data/todoer.db && chown 10001:10001 /data/todoer.db')
        run('up','-d','--wait','--wait-timeout','120')
        def http(method,path,body=None,key=None):
            headers={'Authorization':'Bearer '+token}
            if body is not None:headers['Content-Type']='application/json'
            if key:headers['Idempotency-Key']=key
            request=urllib.request.Request(f'http://127.0.0.1:{port}'+path,data=None if body is None else json.dumps(body).encode(),headers=headers,method=method)
            try:
                with urllib.request.urlopen(request,timeout=20) as response:return response.status,json.load(response)
            except urllib.error.HTTPError as error:return error.code,json.load(error)
        status,capabilities=http('GET','/api/v1/capabilities');assert status==200 and capabilities['production_auth'] is True
        status,_=http('GET','/ready');assert status==200
        payload={'list_id':'smoke-list','tasks':[{'title':'Synthetic container transaction','description':'Private fixture only'}]}
        status,tasks=http('POST','/api/v1/capture',payload,'container-once');assert status==200
        assert http('POST','/api/v1/capture',payload,'container-once')[1]==tasks
        task=tasks[0]
        assert http('PATCH','/api/v1/tasks/'+task['id'],{'revision':task['revision'],'title':'Synthetic container updated'})[0]==200
        assert http('PATCH','/api/v1/tasks/'+task['id'],{'revision':task['revision'],'title':'Stale write'})[0]==409
        assert http('GET','/api/v1/lists/not-authorized/tasks')[0] in (403,404)
        cli=run('exec','-T','-e','TODOER_URL=http://127.0.0.1:8092','-e','TODOER_TOKEN='+token,'app','todoer','--json','subtree',task['id'])
        assert json.loads(cli.stdout)[0]['title']=='Synthetic container updated'
        deadline=time.monotonic()+45
        while time.monotonic()<deadline:
            name='/tmp/restored-'+uuid.uuid4().hex+'.db'
            result=run('exec','-T','litestream','litestream','restore','-config','/etc/litestream.yml','-o',name,'-integrity-check','full','-json','/data/todoer.db',check=False)
            if result.returncode==0:
                restored=temporary/('restored-'+uuid.uuid4().hex+'.db')
                assert json.loads(result.stdout)['db_path']==name, 'restore returned unexpected path'
                copied=subprocess.run([*compose,'exec','-T','litestream','cat',name],env=env,check=True,capture_output=True)
                restored.write_bytes(copied.stdout)
                with sqlite3.connect('file:'+str(restored)+'?mode=ro',uri=True) as db:
                    row=db.execute('SELECT title FROM tasks WHERE id=?',(task['id'],)).fetchone()
                    assert db.execute('PRAGMA integrity_check').fetchone()==('ok',)
                    assert db.execute('PRAGMA foreign_key_check').fetchall()==[]
                if row==('Synthetic container updated',):break
            time.sleep(1)
        else:raise AssertionError('Compose replica did not restore latest committed transaction')
        app=run('ps','-q','app').stdout.strip()
        settings=json.loads(docker('inspect',app).stdout)[0]
        assert settings['Config']['User']=='10001:10001' and settings['HostConfig']['ReadonlyRootfs']
        assert 'TODOER_REPLICA_SECRET_ACCESS_KEY=development' not in settings['Config']['Env']
        print(json.dumps({'check':'production image + Compose + REST + CLI + Litestream isolated restore','architecture':'linux/amd64','production_auth':True,'nonroot':True,'readonly_root':True,'passed':True}))
    finally:
        run('down','--volumes',check=False)
        if relay:relay.shutdown();relay.server_close()
        docker('network','rm',proxy,check=False)
