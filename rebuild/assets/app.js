const $ = (s) => document.querySelector(s);
const csrf = $('meta[name="csrf-token"]').content;
const list = document.body.dataset.list;
const dialog = $('#task-editor');
let editor = null;
let cursor = 0;
let busy = false;
let pendingRefresh = false;
const draftKey = `todoer:dev:draft:${list}`;
const input = $('#capture-title');
input.value = sessionStorage.getItem(draftKey) || '';
input.addEventListener('input', () => sessionStorage.setItem(draftKey, input.value));
function notice(text, error = false) { $('#notice').textContent = text; $('#notice').classList.toggle('error', error); }
async function api(path, method = 'GET', body, extra = {}) {
  const response = await fetch(`/api/v1${path}`, {method, headers: {'Content-Type': 'application/json', 'X-CSRF-Token': csrf, ...extra}, body: body === undefined ? undefined : JSON.stringify(body)});
  if (response.status === 401) { notice('Your session ended. Copy any draft before signing in again.', true); throw new Error('Authentication required'); }
  let data;
  try { data = await response.json(); } catch { throw new Error('Unexpected server response. Your draft is safe.'); }
  if (!response.ok) { const e = new Error(data.error?.message || 'Request failed'); e.status = response.status; throw e; }
  return data;
}
function draftActive() { return dialog.open || input.value.length > 0 || busy || Boolean(window.getSelection()?.toString()); }
function refresh(force = false) {
  if (!force && draftActive()) { pendingRefresh = true; $('#refresh-notice').hidden = false; return; }
  pendingRefresh = false; $('#refresh-notice').hidden = true;
  window.htmx.trigger('#task-list', 'tasks-refresh');
}
$('#refresh-now').onclick = () => { if (dialog.open) notice('Finish or close the task editor first.'); else refresh(true); };
document.body.addEventListener('htmx:afterSwap', (e) => {
  if (e.target.id === 'task-list') $('#task-count').textContent = $$('#task-list .task').length;
});
const $$ = (s) => [...document.querySelectorAll(s)];
document.body.addEventListener('htmx:responseError', () => notice('Could not refresh tasks. Try again.', true));
$('#show-completed').onchange = (e) => $('#task-list').classList.toggle('hide-completed', !e.target.checked);
$('#nav-toggle').onclick = () => { const open = $('#navigation').classList.toggle('open'); $('#nav-toggle').setAttribute('aria-expanded', String(open)); };
$('#quick-add').onclick = () => input.focus();
document.addEventListener('keydown', (e) => { if (e.key === 'n' && !e.ctrlKey && !e.metaKey && !e.altKey && !e.target.closest('input,textarea,[contenteditable="true"]') && !dialog.open) { e.preventDefault(); input.focus(); } });
let capture = null;
$('#capture-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  const title = input.value.trim();
  if (!title || busy) return;
  if (!capture || capture.title !== title) capture = {title, key: crypto.randomUUID()};
  busy = true; const button = e.target.querySelector('button'); button.disabled = true;
  try {
    await api('/capture', 'POST', {list_id:list,tasks:[{title,description:'',parent_id:null}]}, {'Idempotency-Key':capture.key});
    // Do not clear text typed while the request was in flight.
    if (input.value.trim() === title) { input.value = ''; sessionStorage.removeItem(draftKey); }
    capture = null; notice('Task added.'); refresh(true);
  } catch (err) { notice(`${err.message} Retry with Add; the same request will not duplicate.`, true); }
  finally { busy = false; button.disabled = false; input.focus(); }
});
$('#task-list').addEventListener('click', async (e) => {
  const row = e.target.closest('.task'); if (!row) return;
  if (e.target.closest('.completion')) {
    const button = e.target.closest('.completion'); if (button.disabled) return;
    const complete = row.dataset.complete !== 'true';
    button.disabled = true; row.classList.toggle('is-complete', complete); button.setAttribute('aria-pressed', String(complete));
    const previous = button.innerHTML; button.textContent = complete ? '✓' : '';
    try {
      const task = await api(`/tasks/${row.dataset.id}`, 'PATCH', {revision:Number(row.dataset.revision),complete});
      row.dataset.revision = task.revision; row.dataset.complete = task.complete; button.setAttribute('aria-label', `${task.complete ? 'Reopen' : 'Complete'} ${task.title}`);
      notice(task.complete ? 'Task completed.' : 'Task reopened.');
    } catch (err) {
      row.classList.toggle('is-complete', !complete); button.setAttribute('aria-pressed', String(!complete)); button.innerHTML = previous;
      notice(`${err.message} Your action was rolled back.`, true);
      if (err.status === 409) refresh();
    } finally { button.disabled = false; }
  } else if (e.target.closest('[data-action="edit"]')) {
    try {
      const tasks = await api(`/tasks/${row.dataset.id}/subtree`);
      editor = tasks.find(t => t.id === row.dataset.id);
      $('#edit-title').value = editor.title; $('#edit-description').value = editor.description;
      $('#editor-reference').textContent = `%${editor.reference}`; $('#edit-error').textContent = '';
      const saved = sessionStorage.getItem(`todoer:dev:edit:${editor.id}`);
      if (saved) { const draft = JSON.parse(saved); $('#edit-title').value = draft.title; $('#edit-description').value = draft.description; editor.revision = draft.revision; $('#edit-error').textContent = 'Restored your unsaved draft. Saving will check its original revision.'; }
      dialog.showModal(); history.pushState({task:editor.id}, '', `?task=${editor.id}`); $('#edit-title').focus();
    } catch (err) { notice(err.message, true); }
  }
});
function saveEditorDraft() {
  if (editor) sessionStorage.setItem(`todoer:dev:edit:${editor.id}`,JSON.stringify({title:$('#edit-title').value,description:$('#edit-description').value,revision:editor.revision}));
}
$('#edit-title').addEventListener('input', saveEditorDraft); $('#edit-description').addEventListener('input', saveEditorDraft);
function closeEditor() { dialog.close(); if (location.search.includes('task=')) history.back(); if (pendingRefresh) refresh(); }
$('#close-editor').onclick = closeEditor; $('#cancel-edit').onclick = closeEditor;
dialog.addEventListener('cancel', e => { e.preventDefault(); closeEditor(); });
window.addEventListener('popstate', () => { if (dialog.open) dialog.close(); });
$('#edit-form').addEventListener('submit', async (e) => {
  e.preventDefault(); if (!editor) return;
  const button = e.target.querySelector('button[type="submit"]'); button.disabled = true;
  try {
    await api(`/tasks/${editor.id}`, 'PATCH', {revision:editor.revision,title:$('#edit-title').value,description:$('#edit-description').value});
    sessionStorage.removeItem(`todoer:dev:edit:${editor.id}`); closeEditor(); notice('Changes saved.'); refresh(true);
  } catch (err) { $('#edit-error').textContent = `${err.message}. Your draft is preserved. Close to review the latest task; copy your draft before discarding it.`; }
  finally { button.disabled = false; }
});
$('#logout').onclick = async () => {
  const response = await fetch('/logout', {method:'POST',headers:{'X-CSRF-Token':csrf}});
  if (response.ok) { for (const key of Object.keys(sessionStorage)) if (key.startsWith('todoer:')) sessionStorage.removeItem(key); location.href = '/signin'; }
};
async function sync() {
  try {
    const changes = await api(`/changes?after=${cursor}`);
    if (changes.length) { cursor = Math.max(cursor, ...changes.map(c=>c.seq)); refresh(); }
    $('#connection').textContent = 'Up to date';
  } catch { $('#connection').textContent = navigator.onLine ? 'Reconnecting' : 'Offline'; }
  setTimeout(sync, 2000);
}
sync();
// Deep links open the same editor after the initial server render.
const taskId = new URLSearchParams(location.search).get('task');
if (taskId) { const row = $$('.task').find(r=>r.dataset.id===taskId); if (row) { history.replaceState(null,'',location.pathname); row.querySelector('[data-action="edit"]').click(); } }
