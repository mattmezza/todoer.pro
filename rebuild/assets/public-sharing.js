import { confirmAction } from './confirm.js';
import { setSaveState } from './quiet-info.js';
import { toast } from './toast.js';
export async function mountPublicSharing(section, kind, id) {
  const root = document.createElement('section');
  root.className = 'public-sharing-control';
  const h = document.createElement('h3');
  h.textContent = 'Public access';
  root.append(h);
  section.append(root);
  const endpoint = `/api/v1/public-access/${kind}/${encodeURIComponent(id)}`;
  let state;
  async function request(method, body) {
    const response = await fetch(endpoint, {
      method,
      cache: 'no-store',
      headers: {
        'Content-Type': 'application/json',
        'X-CSRF-Token': document.querySelector('meta[name=csrf-token]')?.content || '',
      },
      ...(body ? { body: JSON.stringify(body) } : {}),
    });
    const data = await response.json();
    if (!response.ok)
      throw Error(data.error?.message || 'Could not update public access.');
    return data;
  }
  try {
    state = await request('GET');
    if (!root.isConnected) return;
    draw();
  } catch {
    root.remove();
  }
  function draw() {
    root.replaceChildren(h);
    const text = document.createElement('p');
    text.className = 'muted';
    text.textContent =
      kind === 'space'
        ? 'Anyone with the link can read unrestricted lists, task names, descriptions and completion status. Restricted lists keep their own access.'
        : 'Anyone with the link can read task names, descriptions and completion status. Comments and files require member access.';
    root.append(text);
    const label = document.createElement('label');
    label.className = 'public-sharing-toggle';
    const toggle = document.createElement('input');
    toggle.type = 'checkbox';
    toggle.checked = state.enabled;
    toggle.disabled = !state.can_manage;
    label.append(toggle, 'Anyone with the link can view');
    root.append(label);
    if (state.inherited) {
      const note = document.createElement('p');
      note.className = 'muted';
      note.textContent =
        'Also public through its space. Restrict this list to remove inherited public access.';
      root.append(note);
    }
    if (state.effective) {
      const copy = document.createElement('button');
      copy.type = 'button';
      copy.textContent = 'Copy public link';
      copy.onclick = async () => {
        try {
          await navigator.clipboard.writeText(
            location.origin +
              (kind === 'space' ? '/space/' : '/lists/') +
              state.reference,
          );
          toast('Public link copied.');
        } catch (e) {
          setSaveState('public-access', 'error', 'Could not copy public link.');
        }
      };
      root.append(copy);
    }
    toggle.onchange = async () => {
      const enabled = toggle.checked;
      toggle.checked = state.enabled;
      if (
        enabled &&
        !(await confirmAction(
          kind === 'space'
            ? 'Make this space and its unrestricted lists viewable by anyone with the link? Task descriptions will be public.'
            : 'Make this list and its tasks viewable by anyone with the link? Task descriptions will be public.',
        ))
      )
        return;
      toggle.disabled = true;
      setSaveState('public-access', 'saving');
      try {
        state = await request('PUT', { enabled, revision: state.revision });
        setSaveState('public-access', 'saved');
        toast(enabled ? 'Public access enabled.' : 'Public access disabled.');
        draw();
      } catch (e) {
        setSaveState('public-access', 'error', e.message);
        toggle.disabled = !state.can_manage;
      }
    };
  }
}
