import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { createHash, createSign, randomBytes, randomUUID } from 'node:crypto';
import { createServer } from 'node:http';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StreamableHTTPClientTransport } from '@modelcontextprotocol/sdk/client/streamableHttp.js';

const config = JSON.parse(await readFile(process.argv[2], 'utf8'));
const issuer = `${config.identity}/realms/trust`;
const discovery = await (await fetch(`${issuer}/.well-known/openid-configuration`)).json();
const checks = [...config.infrastructureChecks];
function passed(name) { checks.push(name); console.log(`PASS ${name}`); }
function claims(token) { return JSON.parse(Buffer.from(token.split('.')[1], 'base64url')); }
async function token(fields) {
  const response = await fetch(discovery.token_endpoint, { method: 'POST', body: new URLSearchParams(fields) });
  return { ok: response.ok, body: await response.json() };
}
async function clientAssertion() {
  const now = Math.floor(Date.now()/1000);
  const assertion = [Buffer.from(JSON.stringify({alg:'RS256',typ:'JWT'})).toString('base64url'),
    Buffer.from(JSON.stringify({iss:'trust-agent',sub:'trust-agent',aud:discovery.token_endpoint,
      iat:now,exp:now+60,jti:randomUUID()})).toString('base64url')].join('.');
  const signature = createSign('RSA-SHA256').update(assertion).sign(await readFile(config.agentKey), 'base64url');
  return `${assertion}.${signature}`;
}
function docker(...args) {
  const result = spawnSync('docker', args, {encoding:'utf8',timeout:90000});
  assert.equal(result.status, 0, 'Isolated test container operation must succeed');
  return result.stdout.trim();
}
function database(query) {
  return docker('exec', config.databaseContainer, 'psql', '-U', 'keycloak', '-d', 'keycloak',
    '-v', 'ON_ERROR_STOP=1', '-Atc', query);
}
async function readyAfterRestart() {
  const deadline = Date.now() + 90000;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`${issuer}/.well-known/openid-configuration`, {signal:AbortSignal.timeout(2000)});
      if (response.ok) return;
    } catch { /* Wait for the same isolated provider to finish database recovery. */ }
    await new Promise(resolve => setTimeout(resolve, 250));
  }
  throw new Error('Identity provider did not recover after database restart');
}
function authorization(client, extra = {}) {
  const verifier = randomBytes(32).toString('base64url');
  const state = randomUUID();
  const params = new URLSearchParams({client_id: client, redirect_uri: config.callback, response_type: 'code',
    scope: 'openid', state, code_challenge: createHash('sha256').update(verifier).digest('base64url'),
    code_challenge_method: 'S256', ...extra});
  return {url: `${discovery.authorization_endpoint}?${params}`, verifier, state, client};
}
async function exchange(page, request) {
  await page.waitForURL(url => url.origin + url.pathname === config.callback, { timeout: 20000 });
  const result = new URL(page.url());
  assert.equal(result.searchParams.get('state'), request.state);
  assert.ok(result.searchParams.get('code'), 'Authorization must return a code');
  const response = await token({grant_type: 'authorization_code', client_id: request.client,
    redirect_uri: config.callback, code: result.searchParams.get('code'), code_verifier: request.verifier});
  assert.ok(response.ok, 'Authorization code exchange must succeed');
  return response.body;
}
const browser = await chromium.launch({ headless: true });
const callbackServer=createServer((_request,response)=>{response.writeHead(200,{'Content-Type':'text/plain'});response.end('Local callback');});
await new Promise(resolve=>callbackServer.listen(Number(new URL(config.callback).port),'127.0.0.1',resolve));
const context = await browser.newContext();
const page = await context.newPage();
const cdp = await context.newCDPSession(page);
await cdp.send('WebAuthn.enable');
const {authenticatorId} = await cdp.send('WebAuthn.addVirtualAuthenticator', {options: {
  protocol:'ctap2', ctap2Version:'ctap2_1', transport:'usb', hasResidentKey:true,
  hasUserVerification:true, isUserVerified:true, automaticPresenceSimulation:true}});
page.on('dialog', dialog => dialog.accept('Local test security key'));
async function username(name) {
  await page.locator('#username').fill(name);
  await page.locator('#kc-login').click();
}
try {
  // No credential must fail BEFORE any required-action registration can run.
  const absent = authorization('trust-owner');
  await page.goto(absent.url);
  await username('no-key');
  await page.waitForLoadState('networkidle');
  assert.ok(!page.url().includes('code='), 'Unenrolled user must not receive a code');
  assert.equal(await page.locator('#registerWebAuthn').count(), 0, 'Privileged flow must not offer enrollment');
  assert.match(await page.locator('body').innerText(), /denied|invalid|error|not allowed|Cannot login/i);
  passed('unenrolled owner cannot enroll through privileged flow');

  const enroll = authorization('trust-enrollment');
  await page.goto(enroll.url);
  await page.locator('#username').fill('owner');
  await page.locator('#password').fill(config.password);
  await page.locator('#kc-login').click();
  await page.locator('#registerWebAuthn').click();
  const enrollment = await exchange(page, enroll);
  assert.ok(!JSON.stringify(claims(enrollment.access_token).aud ?? []).includes(config.origin));
  passed('separate enrollment client has no Trust audience');

  const ownerRequest = authorization('trust-owner', {acr_values:'0'});
  await page.goto(ownerRequest.url);
  await username('owner');
  const authenticate = page.locator('#authenticateWebAuthnButton');
  await authenticate.waitFor({state:"visible"});
  await authenticate.click();
  const owner = await exchange(page, ownerRequest);
  const ownerClaims = claims(owner.access_token);
  assert.equal(ownerClaims.sub, config.ownerSubject);
  assert.equal(ownerClaims.acr, 'urn:trust:owner:mfa');
  assert.ok(ownerClaims.auth_time > 0 && ownerClaims.exp - ownerClaims.iat <= 300);
  assert.equal(owner.refresh_token, undefined);
  passed('verified passkey produces recent owner ACR despite requested downgrade; no refresh token');

  const ownerClient = new Client({name:'trust-owner-acceptance', version:'1.0.0'});
  await ownerClient.connect(new StreamableHTTPClientTransport(new URL(`${config.origin}/mcp`),
    {requestInit:{headers:{Authorization:`Bearer ${owner.access_token}`}}}));
  const ownerTools = await ownerClient.listTools();
  assert.ok(ownerTools.tools.some(tool => tool.name === 'account_safety_policy_set'));
  const created = await ownerClient.callTool({name:'account_create', arguments:{name:'OIDC paper', description:'Local authentication test', environment:'paper', broker:'ibkr', broker_account_id:'DU_OIDC', idempotency_key:randomUUID()}});
  assert.ok(!created.isError, 'Owner key must authorize account creation');
  passed('official MCP SDK discovers owner tools and performs audited owner mutation');
  await cdp.send('WebAuthn.setUserVerified',{authenticatorId,isUserVerified:false});
  const noVerification=authorization('trust-owner');
  await page.goto(noVerification.url);
  await username('owner');
  await authenticate.waitFor({state:"visible"});
  await authenticate.click();
  await page.waitForLoadState('networkidle');
  assert.ok(!page.url().startsWith(config.callback),'A key without user verification must not authorize');
  assert.match(await page.locator('body').innerText(),/invalid|error|failed|not authenticated/i);
  await cdp.send('WebAuthn.setUserVerified',{authenticatorId,isUserVerified:true});
  passed('security key assertion without user verification is denied despite an existing session');
  const missingPkce=authorization('trust-owner');
  const unprotected=new URL(missingPkce.url);unprotected.searchParams.delete('code_challenge');unprotected.searchParams.delete('code_challenge_method');
  await page.goto(unprotected.toString());
  assert.ok(!new URL(page.url()).searchParams.get('code'));
  assert.ok(new URL(page.url()).searchParams.has('error')||/invalid|error|required/i.test(await page.locator('body').innerText()));
  passed('authorization code flow requires PKCE S256');

  const assertion = await clientAssertion();
  const agent = await token({grant_type:'client_credentials',client_id:'trust-agent',
    client_assertion_type:'urn:ietf:params:oauth:client-assertion-type:jwt-bearer', client_assertion:assertion});
  assert.ok(agent.ok, 'Agent private_key_jwt must authenticate');
  const agentClaims = claims(agent.body.access_token);
  assert.equal(agentClaims.sub, config.agentSubject);
  assert.ok(!agentClaims.scope.includes('trust:policy') && !agentClaims.scope.includes('trust:admin'));
  assert.notEqual(agentClaims.acr, 'urn:trust:owner:mfa');
  const assertionReplay=await token({grant_type:'client_credentials',client_id:'trust-agent',client_assertion_type:'urn:ietf:params:oauth:client-assertion-type:jwt-bearer',client_assertion:assertion,scope:'trust:policy trust:admin'});
  assert.equal(assertionReplay.ok,false);
  passed('replayed signed client assertion cannot mint another agent or owner token');
  const agentClient = new Client({name:'trust-agent-acceptance',version:'1.0.0'});
  await agentClient.connect(new StreamableHTTPClientTransport(new URL(`${config.origin}/mcp`),
    {requestInit:{headers:{Authorization:`Bearer ${agent.body.access_token}`}}}));
  const agentTools = await agentClient.listTools();
  assert.ok(!agentTools.tools.some(tool => tool.name === 'account_safety_policy_set'));
  const denied = await agentClient.callTool({name:'account_create',arguments:{name:'Forbidden',description:'Denied',environment:'paper',broker:'ibkr',broker_account_id:'DU_OIDC',idempotency_key:randomUUID()}});
  assert.equal(denied.isError, true);
  passed('private_key_jwt agent discovers restricted tools and cannot perform owner mutation');
  await writeFile(config.sdkCredentials,JSON.stringify({url:`${config.origin}/mcp`,token:agent.body.access_token}),{mode:0o600});
  const python=spawnSync('uv',['run','--locked','python','python_client.py',config.sdkCredentials],{cwd:fileURLToPath(new URL('.',import.meta.url)),stdio:'inherit'});
  assert.equal(python.status,0,'Independent Python SDK compatibility');
  passed('second MCP client implementation accepts current protocol and result contracts');

  const password = await token({grant_type:'password',client_id:'trust-owner',username:'owner',password:config.password});
  assert.equal(password.ok, false);
  const refresh = await token({grant_type:'refresh_token',client_id:'trust-owner',refresh_token:owner.access_token});
  assert.equal(refresh.ok, false);
  passed('password and refresh grants cannot create owner authority');

  // The service's deliberately shortened window tests expiry independently of token expiry.
  const remaining = Math.max(0, ownerClaims.auth_time + 16 - Math.floor(Date.now()/1000));
  await new Promise(resolve => setTimeout(resolve, remaining * 1000));
  const expired = await fetch(`${config.origin}/mcp`, {method:'POST', headers:{Authorization:`Bearer ${owner.access_token}`,'Content-Type':'application/json','Accept':'application/json, text/event-stream'},body:JSON.stringify({jsonrpc:'2.0',id:1,method:'tools/list',params:{}})});
  assert.equal(expired.status, 401);
  assert.match(expired.headers.get('www-authenticate'), /insufficient_user_authentication/);
  passed('owner session expires by original auth_time while access token is still valid');
  await ownerClient.close();
  await agentClient.close();

  // Stop the provider first to discard its caches. Restart PostgreSQL on its
  // existing volume, then require a new real passkey ceremony and signed grant.
  const jwks = await (await fetch(discovery.jwks_uri)).json();
  assert.equal(database("SELECT count(*) FROM credential WHERE type='webauthn-passwordless'"), '1');
  docker('stop', '--time=30', config.identityContainer);
  docker('restart', '--time=30', config.databaseContainer);
  docker('start', config.identityContainer);
  await readyAfterRestart();
  const restoredJwks = await (await fetch(discovery.jwks_uri)).json();
  assert.deepEqual(restoredJwks.keys.map(key=>key.kid).sort(), jwks.keys.map(key=>key.kid).sort());
  assert.equal(database("SELECT count(*) FROM credential WHERE type='webauthn-passwordless'"), '1');
  assert.equal(database('SELECT sum(value) FROM trust_acceptance'), '1');
  await context.clearCookies();
  const restoredRequest = authorization('trust-owner');
  await page.goto(restoredRequest.url);
  await username('owner');
  await authenticate.waitFor({state:'visible'});
  await authenticate.click();
  const restoredOwner = await exchange(page, restoredRequest);
  assert.equal(claims(restoredOwner.access_token).sub, config.ownerSubject);
  assert.equal(claims(restoredOwner.access_token).acr, 'urn:trust:owner:mfa');
  const restoredClient = new Client({name:'trust-owner-restart-acceptance',version:'1.0.0'});
  await restoredClient.connect(new StreamableHTTPClientTransport(new URL(`${config.origin}/mcp`),
    {requestInit:{headers:{Authorization:`Bearer ${restoredOwner.access_token}`}}}));
  const restoredCreated = await restoredClient.callTool({name:'account_create',arguments:{
    name:'OIDC restart',description:'PostgreSQL restart acceptance',environment:'paper',broker:'ibkr',
    broker_account_id:'DU_OIDC_RESTART',idempotency_key:randomUUID()}});
  assert.ok(!restoredCreated.isError, 'New passkey ceremony after restart must authorize owner mutation');
  await restoredClient.close();
  const restoredAgent = await token({grant_type:'client_credentials',client_id:'trust-agent',
    client_assertion_type:'urn:ietf:params:oauth:client-assertion-type:jwt-bearer',client_assertion:await clientAssertion()});
  assert.ok(restoredAgent.ok, 'Private-key agent must authenticate after database restart');
  assert.equal(claims(restoredAgent.body.access_token).sub, config.agentSubject);
  await writeFile(config.sdkCredentials,JSON.stringify({url:`${config.origin}/mcp`,token:restoredAgent.body.access_token}),{mode:0o600});
  const restoredPython = spawnSync('uv',['run','--locked','python','python_client.py',config.sdkCredentials],
    {cwd:fileURLToPath(new URL('.',import.meta.url)),stdio:'inherit'});
  assert.equal(restoredPython.status,0,'Fresh agent grant must work through the independent MCP client after restart');
  passed('PostgreSQL restart preserves committed data, signing keys, passkeys and agent authentication');
  await writeFile(`${config.artifacts}/evidence.json`, JSON.stringify({provider:'Keycloak 26.8.1',database:'PostgreSQL 18.6',images:config.images,clients:['MCP TypeScript SDK 1.32.2','MCP Python SDK 2.3.0'],browser:'Chromium virtual CTAP2 authenticator',checks,physical_key_verified:false,production_deployment_verified:false},null,2)+'\n');
} catch (error) {
  // Screenshots omit token responses; no browser traces containing auth codes or credentials.
  await page.screenshot({path:`${config.artifacts}/failure.png`});
  console.error(`FAIL ${error.message.split("\n")[0].replace(/https?:\/\/\S+/g,"[URL omitted]")}`);
  process.exitCode=1;
} finally { await browser.close(); await new Promise(resolve=>callbackServer.close(resolve)); }
