import {request as fetch} from './request.js';
import {passkey} from './passkeys.js';
const form = document.querySelector('[data-auth-form]');
const notice = document.querySelector('[data-auth-notice]');
const keyButton = document.querySelector('[data-auth-passkey]');
let busy = false;
let token = new URLSearchParams(location.hash.slice(1)).get('token') || '';
// Recovery bearer tokens stay out of URLs, referrers and browser navigation history.
if (token) history.replaceState(null, '', location.pathname + location.search);
const params = new URLSearchParams(location.search);
async function post(url, data) {
  const response = await fetch(url, {method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(data)});
  const result = await response.json();
  if (!response.ok) throw Error(result.error?.message || 'Could not complete this request.');
  return result;
}
async function action(fn) {
  if (busy) return;
  busy = true; notice.textContent = ''; notice.classList.remove('error');
  document.querySelectorAll('.auth-card button').forEach(b=>b.disabled=true);
  try { await fn(); } catch(error) { notice.textContent = error.name === 'NotAllowedError' ? 'Passkey request cancelled or unavailable. Try another sign-in method.' : error.message; notice.classList.add('error'); }
  finally {busy=false;document.querySelectorAll('.auth-card button').forEach(b=>b.disabled=false);}
}
form?.addEventListener('submit', event=>{
  event.preventDefault();
  action(async()=>{
    const values=Object.fromEntries(new FormData(form));
    values.next=params.get('next') || '/today';
    if (['verify','reset'].includes(form.dataset.authForm)) {
      if (!token) throw Error('Open the complete link from your email. It may have expired.');
      values.token=token;
    }
    const result=await post(`/auth/local/${form.dataset.authForm}`,values);
    if(result.mfa){location.assign('/signin?step=mfa');return;}
    if(result.redirect){location.assign(result.redirect);return;}
    notice.textContent=result.message || '';
    if(form.dataset.authForm==='register'||form.dataset.authForm==='forgot')form.reset();
  });
});
keyButton?.addEventListener('click',()=>action(async()=>{
  const email=form?.elements.email?.value || '';
  const reset=form?.dataset.authForm==='reset';
  if(!email&&!reset&&params.get('step')!=='mfa')throw Error('Enter your email first, then select passkey.');
  const start=await post('/auth/passkey/start',{email,...(reset?{reset_token:token}:{})});
  const credential=await passkey(start.options);
  const result=await post('/auth/passkey/finish',{challenge_id:start.challenge_id,credential});
  if(result.redirect)location.assign(result.redirect);
  else if(result.verified)notice.textContent='Passkey verified. Save your new password now.';
}));
