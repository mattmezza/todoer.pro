import { stowInfo } from './quiet-info.js';
import { confirmAction } from './confirm.js';
// Task-local controller. Parent must call its cleanup before replacing the panel.
export function mountDiscussion({ taskId, csrf, container, accountId, onConflict }) {
  if (!container || !taskId || !accountId)
    throw new Error('Discussion requires a task, account and container');
  const prefix = `todoer:${encodeURIComponent(accountId)}:discussion:${encodeURIComponent(taskId)}:`;
  const requests = new Set();
  const busy = new Set();
  let active = true;
  let loading = false;
  let pendingRefresh = false;
  let storageWorks = true;
  let xhr = null;
  let hashing = false;
  let selectionVersion = 0;
  let fileState = null;
  let newDraftState = read('new');
  const get = (selector) => container.querySelector(selector);
  const all = (selector) => [...container.querySelectorAll(selector)];
  function read(key) {
    try {
      return JSON.parse(sessionStorage.getItem(prefix + key) || 'null');
    } catch {
      return null;
    }
  }
  function save(key, value) {
    if (!active) return;
    if (key === 'new') newDraftState = value;
    try {
      sessionStorage.setItem(prefix + key, JSON.stringify(value));
    } catch {
      storageWorks = false;
      notice(
        'Browser draft storage is unavailable. Copy your edits before closing this panel.',
        true,
      );
    }
  }
  function remove(key) {
    if (!active) return;
    if (key === 'new') newDraftState = null;
    try {
      sessionStorage.removeItem(prefix + key);
    } catch {}
  }
  function notice(message, error = false) {
    if (!active) return;
    if (!error && stowInfo(message, `discussion:${taskId}`)) {
      const target = get('[data-discussion-notice]');
      if (target) target.textContent = '';
      return;
    }
    let target = get('[data-discussion-notice]');
    if (!target) {
      target = document.createElement('p');
      target.dataset.discussionNotice = '';
      target.setAttribute('role', 'status');
      target.setAttribute('aria-live', 'polite');
      container.prepend(target);
    }
    target.textContent = message;
    target.classList.toggle('error', error);
  }
  async function request(path, method = 'GET', body, html = false, extraHeaders = {}) {
    if (!active) throw new Error('Discussion panel closed');
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 300000);
    requests.add(controller);
    try {
      const response = await fetch(path, {
        method,
        signal: controller.signal,
        headers: {
          'X-CSRF-Token': csrf,
          ...(body === undefined ? {} : { 'Content-Type': 'application/json' }),
          ...extraHeaders,
        },
        body: body === undefined ? undefined : JSON.stringify(body),
        cache: 'no-store',
      });
      if (!response.ok) {
        const data = await response.json().catch(() => null);
        const error = new Error(
          data?.error?.message || `Discussion request failed (${response.status})`,
        );
        error.status = response.status;
        if (response.status === 409) onConflict?.(error);
        throw error;
      }
      return html ? await response.text() : await response.json();
    } finally {
      clearTimeout(timeout);
      requests.delete(controller);
    }
  }
  // HTML comes solely from the authenticated, sanitized server fragment renderer.
  function parsed(html) {
    const template = document.createElement('template');
    template.innerHTML = html;
    const root = template.content.querySelector('[data-discussion-fragment]');
    if (!root) throw new Error('Discussion could not be rendered');
    return root;
  }
  function editKey(id) {
    return `edit:${id}`;
  }
  function draftsActive() {
    return Boolean(
      container.contains(document.activeElement) ||
      get('[data-comment-composer]')?.open ||
      get('[data-new-comment-input]')?.value ||
      all('[data-comment-form]').some((form) => !form.hidden),
    );
  }
  function recoverableDrafts() {
    const records = [];
    try {
      for (const key of Object.keys(sessionStorage)) {
        if (key.startsWith(prefix + 'edit:'))
          records.push([
            key.slice(prefix.length),
            JSON.parse(sessionStorage.getItem(key)),
          ]);
      }
    } catch {}
    return records;
  }
  function draftRecovery(key, draft, title) {
    if (!draft?.body) return;
    const details = document.createElement('details');
    details.dataset.recovery = '';
    const summary = document.createElement('summary');
    summary.textContent = title;
    summary.className = 'min-h-11 cursor-pointer';
    const text = document.createElement('textarea');
    text.value = draft.body;
    text.readOnly = true;
    text.rows = 4;
    text.className = 'w-full';
    text.setAttribute('aria-label', title);
    const discard = document.createElement('button');
    discard.type = 'button';
    discard.className = 'min-h-11';
    discard.textContent = 'Discard this local draft';
    discard.onclick = async () => {
      if (await confirmAction('Discard this unsaved local draft?')) {
        remove(key);
        details.remove();
      }
    };
    details.append(summary, text, discard);
    container.append(details);
  }
  function formatTime(time) {
    const date = new Date(Number(time.dataset.createdAt) * 1000);
    const seconds = Math.round((date.getTime() - Date.now()) / 1000);
    const [divisor, unit] =
      Math.abs(seconds) < 3600
        ? [60, 'minute']
        : Math.abs(seconds) < 86400
          ? [3600, 'hour']
          : Math.abs(seconds) < 2592000
            ? [86400, 'day']
            : Math.abs(seconds) < 31536000
              ? [2592000, 'month']
              : [31536000, 'year'];
    time.textContent =
      Math.abs(seconds) < 60
        ? 'now'
        : new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' }).format(
            Math.round(seconds / divisor),
            unit,
          );
    time.title = date.toLocaleString();
    time.dateTime = date.toISOString();
  }
  function restore() {
    for (const time of all('[data-created-at]')) {
      formatTime(time);
    }
    for (const image of all('[data-comment-avatar]'))
      image.onerror = () => image.remove();
    const fresh = get('[data-new-comment-input]');
    const newDraft = newDraftState;
    if (fresh) {
      fresh.value = newDraft?.body || '';
      const composer = get('[data-comment-composer]');
      if (composer) composer.open = Boolean(newDraft?.body);
    } else
      draftRecovery('new', newDraft, 'Unsent comment draft — copy before discarding');
    for (const [key, draft] of recoverableDrafts()) {
      const id = key.slice(5);
      const row = all('[data-comment-id]').find((row) => row.dataset.commentId === id);
      const form = row?.querySelector('[data-comment-form]');
      if (!form) {
        draftRecovery(
          key,
          draft,
          'Unsaved edit for a comment no longer editable — copy before discarding',
        );
        continue;
      }
      form.querySelector('[data-comment-input]').value = draft.body;
      form.dataset.revision = draft.revision;
      form.hidden = false;
      row.querySelector('[data-comment-rendered]').hidden = true;
    }
    if (fileState) updateFileUi();
  }
  async function refresh() {
    if (!active || loading) return;
    if (busy.size || draftsActive() || fileState?.busy || hashing) {
      pendingRefresh = true;

      return;
    }
    loading = true;
    try {
      const markup = await request(
        `/fragments/tasks/${encodeURIComponent(taskId)}/discussion`,
        'GET',
        undefined,
        true,
      );
      if (!active) return;
      // A draft can begin while the fragment is in flight. Never swap it away.
      if (busy.size || draftsActive() || fileState?.busy || hashing) {
        pendingRefresh = true;
        return;
      }
      const root = parsed(markup);
      container.replaceChildren(root);
      pendingRefresh = false;
      restore();
      requestAnimationFrame(growComments);
    } catch (error) {
      if (active) {
        notice(`${error.message}. Your local drafts have not been removed.`, true);
        if (!get('[data-discussion-fragment]')) {
          for (const prior of all('[data-recovery]')) prior.remove();
          draftRecovery(
            'new',
            newDraftState,
            'Unsent comment draft — copy before discarding',
          );
          for (const [key, draft] of recoverableDrafts())
            draftRecovery(key, draft, 'Unsaved comment edit — copy before discarding');
        }
      }
    } finally {
      loading = false;
    }
  }
  function stashEdit(row) {
    const form = row.querySelector('[data-comment-form]');
    const draft = {
      body: form.querySelector('[data-comment-input]').value,
      revision: Number(form.dataset.revision || row.dataset.commentRevision),
    };
    save(editKey(row.dataset.commentId), draft);
  }
  function disable(form, disabled) {
    for (const control of form.querySelectorAll('button,input,textarea,select'))
      control.disabled = disabled;
  }
  async function mutation(key, form, operation) {
    if (busy.has(key)) return;
    busy.add(key);
    if (form) disable(form, true);
    try {
      await operation();
    } catch (error) {
      if (active)
        notice(
          `${error.message}. ${storageWorks ? 'Your unsaved text remains on this device.' : 'Copy unsaved text before closing.'}`,
          true,
        );
    } finally {
      busy.delete(key);
      if (active && form?.isConnected) disable(form, false);
      if (active && pendingRefresh && !draftsActive() && !busy.size && !fileState?.busy)
        refresh();
    }
  }
  function updateFileUi() {
    const form = get('[data-file-form]');
    if (!form) return;
    const description = form.querySelector('[data-file-selection]');
    const button = form.querySelector('[data-file-upload]');
    const target = form.querySelector('[data-file-target]');
    const abandon = form.querySelector('[data-file-abandon]');
    if (!fileState) {
      description.textContent = hashing ? 'Calculating file checksum…' : '';
      button.textContent = 'Upload file';
      button.disabled = hashing;
      target.disabled = false;
      abandon.hidden = true;
      return;
    }
    description.textContent = fileState.file
      ? `${fileState.name} · ${fileState.size} bytes`
      : `Reselect ${fileState.name} (${fileState.size} bytes) to resume its saved upload intent.`;
    target.value = fileState.commentId || '';
    target.disabled = Boolean(fileState.intent) || fileState.busy;
    button.textContent =
      fileState.phase === 'finalize'
        ? 'Retry finalization'
        : fileState.intent
          ? 'Retry upload'
          : 'Upload file';
    button.disabled = Boolean(fileState.busy) || hashing;
    abandon.hidden = false;
    abandon.disabled = Boolean(fileState.busy);
    form.querySelector('[data-file-input]').disabled = Boolean(fileState.busy);
  }
  function persistFile() {
    if (!fileState) return;
    const { name, size, sha256, intent, phase, commentId } = fileState;
    save('upload', { name, size, sha256, intent, phase, commentId });
  }
  async function digest(file) {
    if (file.size > 25 * 1024 * 1024)
      throw new Error('Files must be no larger than 25 MiB');
    const bytes = await crypto.subtle.digest('SHA-256', await file.arrayBuffer());
    return [...new Uint8Array(bytes)]
      .map((byte) => byte.toString(16).padStart(2, '0'))
      .join('');
  }
  async function chooseFile(file) {
    if (!file) return;
    const version = ++selectionVersion;
    hashing = true;
    updateFileUi();
    let hash;
    try {
      hash = await digest(file);
    } finally {
      if (version === selectionVersion) {
        hashing = false;
        if (active) updateFileUi();
      }
    }
    if (!active || version !== selectionVersion) return;
    if (
      fileState?.intent &&
      (hash !== fileState.sha256 ||
        file.size !== fileState.size ||
        file.name !== fileState.name)
    ) {
      if (
        !(await confirmAction(
          'Use a different file? The previous unfinished upload will remain in storage until retention cleanup.',
        ))
      ) {
        const input = get('[data-file-input]');
        if (input) input.value = '';
        return;
      }
      remove('upload');
      fileState = null;
    }
    if (fileState?.intent) fileState.file = file;
    else
      fileState = {
        file,
        name: file.name,
        size: file.size,
        sha256: hash,
        intent: null,
        phase: 'upload',
        commentId: get('[data-file-target]')?.value || null,
      };
    persistFile();
    updateFileUi();
  }
  function uploadBytes(intent, file) {
    return new Promise((resolve, reject) => {
      const upload = new XMLHttpRequest();
      xhr = upload;
      upload.open(
        'PUT',
        `/api/v1/uploads/${encodeURIComponent(intent.id)}/content?revision=${intent.revision}`,
      );
      upload.timeout = 180000;
      upload.setRequestHeader('X-CSRF-Token', csrf);
      upload.setRequestHeader('Content-Type', 'application/octet-stream');
      upload.upload.onprogress = (event) => {
        if (!active) return;
        const progress = get('[data-upload-progress]');
        if (progress) {
          progress.hidden = false;
          if (event.lengthComputable) progress.value = (event.loaded / event.total) * 100;
        }
      };
      upload.onload = () => {
        xhr = null;
        if (upload.status >= 200 && upload.status < 300) resolve();
        else {
          let message = `Upload failed (${upload.status})`;
          try {
            message = JSON.parse(upload.responseText).error.message;
          } catch {}
          const error = new Error(message);
          error.status = upload.status;
          reject(error);
        }
      };
      upload.onerror = upload.ontimeout = () => {
        xhr = null;
        reject(
          new Error('Upload connection interrupted; retry uses the same upload intent'),
        );
      };
      upload.onabort = () => {
        xhr = null;
        reject(new Error('Upload paused; reselect the same file to resume'));
      };
      upload.send(file);
    });
  }
  async function sendFile() {
    if (fileState?.busy) return;
    if (hashing) {
      notice('Calculating the file checksum. Try again when it finishes.');
      return;
    }
    if (!fileState?.file && fileState?.phase !== 'finalize') {
      notice('Select or reselect the same file first.', true);
      return;
    }
    const state = fileState;
    if (!state) return;
    state.busy = true;
    updateFileUi();
    try {
      if (!state.intent) {
        state.commentId = get('[data-file-target]')?.value || null;
        const result = await request(
          `/api/v1/tasks/${encodeURIComponent(taskId)}/uploads`,
          'POST',
          {
            filename: state.name,
            size: state.size,
            sha256: state.sha256,
            comment_id: state.commentId,
          },
        );
        if (!active) return;
        state.intent = result.intent;
        persistFile();
      }
      if (state.phase !== 'finalize') {
        await uploadBytes(state.intent, state.file);
        if (!active) return;
        state.phase = 'finalize';
        persistFile();
      }
      await request(
        `/api/v1/uploads/${encodeURIComponent(state.intent.id)}/finalize`,
        'POST',
        { revision: state.intent.revision },
      );
      if (!active) return;
      remove('upload');
      fileState = null;
      pendingRefresh = true;
      notice('');
    } catch (error) {
      persistFile();
      if (active)
        notice(
          `${error.message}. ${storageWorks ? 'The upload intent is saved on this device' : 'The intent remains in memory only while this panel stays open'}; retry uses the same bytes and intent. If the intent expired, start a new upload and reselect the file.`,
          true,
        );
      if (error.status === 409) onConflict?.(error);
    } finally {
      state.busy = false;
      if (active) {
        updateFileUi();
        if (pendingRefresh && !draftsActive() && !busy.size) refresh();
      }
    }
  }
  function growComments() {
    for (const input of container.querySelectorAll('textarea:not([hidden])')) {
      input.style.height = 'auto';
      input.style.height = `${input.scrollHeight + input.offsetHeight - input.clientHeight}px`;
    }
  }
  function onComposerToggle(event) {
    growComments();
    if (event.target.matches('[data-comment-composer]') && event.target.open)
      event.target.querySelector('[data-new-comment-input]')?.focus();
  }
  container.addEventListener('toggle', onComposerToggle, true);
  function onInput(event) {
    if (event.target.matches('textarea')) growComments();
    if (event.target.matches('[data-new-comment-input]'))
      save('new', { ...newDraftState, body: event.target.value });
    else if (event.target.matches('[data-comment-input]'))
      stashEdit(event.target.closest('[data-comment-id]'));
  }
  async function onChange(event) {
    if (event.target.matches('[data-file-input]')) {
      try {
        await chooseFile(event.target.files[0]);
      } catch (error) {
        notice(error.message, true);
      }
    } else if (
      event.target.matches('[data-file-target]') &&
      fileState &&
      !fileState.intent
    ) {
      fileState.commentId = event.target.value || null;
      persistFile();
    }
  }
  function onSubmit(event) {
    const form = event.target;
    if (!form.matches('[data-new-comment],[data-comment-form],[data-file-form]')) return;
    event.preventDefault();
    event.stopPropagation();
    if (form.matches('[data-file-form]')) {
      sendFile();
      return;
    }
    const input = form.querySelector('textarea');
    const body = input.value;
    if (!body.trim() || new TextEncoder().encode(body).length > 100000) {
      notice('Comments must contain 1–100000 bytes.', true);
      return;
    }
    if (form.matches('[data-new-comment]')) {
      const existing = newDraftState || {};
      if (existing.submittedBody !== undefined && existing.submittedBody !== body) {
        notice(
          'A previous submission may have saved. Restore and retry its original text with the same key, or check the discussion and explicitly discard the local draft before starting a different comment.',
          true,
        );
        return;
      }
      const draft = {
        body,
        key: existing.key || crypto.randomUUID(),
        submittedBody: body,
      };
      save('new', draft);
      mutation('new', form, async () => {
        await request(
          `/api/v1/tasks/${encodeURIComponent(taskId)}/comments`,
          'POST',
          { body },
          false,
          { 'Idempotency-Key': draft.key },
        );
        remove('new');
        input.value = '';
        const composer = get('[data-comment-composer]');
        if (composer) composer.open = false;
        pendingRefresh = true;
        notice('');
      });
    } else {
      const row = form.closest('[data-comment-id]');
      stashEdit(row);
      mutation(row.dataset.commentId, form, async () => {
        await request(
          `/api/v1/comments/${encodeURIComponent(row.dataset.commentId)}`,
          'PATCH',
          {
            body,
            revision: Number(form.dataset.revision || row.dataset.commentRevision),
          },
        );
        remove(editKey(row.dataset.commentId));
        form.hidden = true;
        row.querySelector('[data-comment-rendered]').hidden = false;
        pendingRefresh = true;
        notice('');
      });
    }
  }
  async function onClick(event) {
    const button = event.target.closest('button');
    if (!button) return;
    if (button.matches('[data-comment-composer-close]')) {
      const composer = get('[data-comment-composer]');
      if (composer) composer.open = false;
      return;
    }
    if (button.matches('[data-discussion-refresh]')) {
      await refresh();
      return;
    }
    if (button.matches('[data-new-comment-original]')) {
      const draft = newDraftState;
      if (!draft || draft.submittedBody === undefined) {
        notice('This draft has not been submitted yet.');
        return;
      }
      if (
        !(await confirmAction(
          'Replace the current local text with the originally submitted comment?',
        ))
      )
        return;
      const input = get('[data-new-comment-input]');
      input.value = draft.submittedBody;
      save('new', { ...draft, body: draft.submittedBody });
      input.focus();
      return;
    }
    if (button.matches('[data-new-comment-reset]')) {
      if (busy.has('new')) return;
      if (
        !(await confirmAction(
          'Discard this local comment draft? A previously submitted comment may already be saved. Check the discussion before posting a different comment.',
        ))
      )
        return;
      remove('new');
      const input = get('[data-new-comment-input]');
      if (input) input.value = '';
      if (pendingRefresh) refresh();
      return;
    }
    if (button.matches('[data-file-abandon]')) {
      if (fileState?.busy) return;
      if (
        fileState?.intent &&
        !(await confirmAction(
          'Discard this local upload intent? Storage bytes remain until retention cleanup.',
        ))
      )
        return;
      remove('upload');
      fileState = null;
      const input = get('[data-file-input]');
      if (input) input.value = '';
      updateFileUi();
      return;
    }
    button.closest('.comment-actions')?.removeAttribute('open');
    const row = button.closest('[data-comment-id]');
    if (row && busy.has(row.dataset.commentId)) return;
    if (button.matches('[data-comment-edit]')) {
      const form = row.querySelector('[data-comment-form]');
      if (!form.hidden) return;
      form.dataset.revision = row.dataset.commentRevision;
      form.hidden = false;
      row.querySelector('[data-comment-rendered]').hidden = true;
      form.querySelector('textarea').focus();
      return;
    }
    if (button.matches('[data-comment-cancel]')) {
      if (busy.has(row.dataset.commentId)) return;
      remove(editKey(row.dataset.commentId));
      const form = row.querySelector('[data-comment-form]');
      form.hidden = true;
      form.querySelector('textarea').value = row.querySelector(
        '[data-comment-source]',
      ).value;
      row.querySelector('[data-comment-rendered]').hidden = false;
      if (pendingRefresh) refresh();
      return;
    }
    if (button.matches('[data-comment-delete]')) {
      if (
        !(await confirmAction(
          'Delete this comment and hide its files? Its audit history is retained.',
        ))
      )
        return;
      mutation(row.dataset.commentId, row.querySelector('form'), async () => {
        await request(
          `/api/v1/comments/${encodeURIComponent(row.dataset.commentId)}/delete`,
          'POST',
          { revision: Number(row.dataset.commentRevision) },
        );
        remove(editKey(row.dataset.commentId));
        row.remove();
        pendingRefresh = true;
        notice('');
      });
      return;
    }
    if (button.matches('[data-file-delete]')) {
      if (
        !(await confirmAction(
          'Remove this file from the task? Stored bytes remain under the retention policy.',
        ))
      )
        return;
      const id = button.dataset.fileDelete;
      button.disabled = true;
      await mutation(`file:${id}`, null, async () => {
        await request(`/api/v1/attachments/${encodeURIComponent(id)}/delete`, 'POST', {
          revision: Number(button.dataset.fileRevision),
        });
        button.closest('[data-file-row]').remove();
        notice('');
      });
      if (button.isConnected) button.disabled = false;
      return;
    }
    if (button.matches('[data-more-comments]')) {
      button.disabled = true;
      try {
        const markup = await request(
          `/fragments/tasks/${encodeURIComponent(taskId)}/discussion?after_seq=${Number(button.dataset.moreComments)}`,
          'GET',
          undefined,
          true,
        );
        if (!active) return;
        const root = parsed(markup);
        const items = root.querySelector('[data-comment-items]');
        const target = get('[data-comment-items]');
        button.remove();
        target.append(...items.childNodes);
        const select = get('[data-file-target]');
        if (select)
          for (const article of all('[data-comment-id]'))
            if (
              ![...select.options].some(
                (option) => option.value === article.dataset.commentId,
              )
            ) {
              const option = document.createElement('option');
              option.value = article.dataset.commentId;
              option.textContent = `Comment ${article.dataset.commentSeq}`;
              select.append(option);
            }
        for (const time of target.querySelectorAll('[data-created-at]')) {
          formatTime(time);
        }
      } catch (error) {
        if (active) notice(error.message, true);
      } finally {
        if (active && button.isConnected) button.disabled = false;
      }
    }
  }
  function onKey(event) {
    if (
      event.key === 'Enter' &&
      (event.metaKey || event.ctrlKey) &&
      event.target.matches('[data-new-comment-input],[data-comment-input]')
    ) {
      event.preventDefault();
      event.stopPropagation();
      event.target.closest('form').requestSubmit();
    } else if (
      event.key === 'Escape' &&
      event.target.matches('[data-new-comment-input]')
    ) {
      event.preventDefault();
      event.stopPropagation();
      const composer = get('[data-comment-composer]');
      if (composer) composer.open = false;
    } else if (event.key === 'Escape' && event.target.matches('[data-comment-input]')) {
      // Keep the modal open: Escape on an edit is an explicit local edit cancel.
      event.preventDefault();
      event.stopPropagation();
      event.target.closest('form').querySelector('[data-comment-cancel]').click();
    }
  }
  const savedUpload = read('upload');
  if (savedUpload?.intent) fileState = { ...savedUpload, file: null, busy: false };
  container.addEventListener('input', onInput);
  container.addEventListener('change', onChange);
  container.addEventListener('submit', onSubmit);
  container.addEventListener('click', onClick);
  container.addEventListener('keydown', onKey);
  refresh();
  const cleanup = () => {
    const input = get('[data-new-comment-input]');
    if (input?.value) save('new', { ...newDraftState, body: input.value });
    for (const row of all('[data-comment-id]'))
      if (
        !row.querySelector('[data-comment-form]')?.hidden &&
        row.querySelector('[data-comment-form]')
      )
        stashEdit(row);
    persistFile();
    active = false;
    selectionVersion++;
    for (const controller of requests) controller.abort();
    xhr?.abort();
    container.removeEventListener('toggle', onComposerToggle, true);
    container.removeEventListener('input', onInput);
    container.removeEventListener('change', onChange);
    container.removeEventListener('submit', onSubmit);
    container.removeEventListener('click', onClick);
    container.removeEventListener('keydown', onKey);
  };
  cleanup.refresh = refresh;
  return cleanup;
}
