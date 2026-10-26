let region;
let timer;
let announce;
export function toast(message) {
  if (!region) {
    region = document.createElement('div');
    region.className = 'toast';
    region.setAttribute('role', 'status');
    region.setAttribute('aria-live', 'polite');
    region.setAttribute('aria-atomic', 'true');
    document.body.append(region);
  }
  clearTimeout(timer);
  clearTimeout(announce);
  region.textContent = '';
  region.classList.remove('is-visible');
  announce = setTimeout(() => {
    region.textContent = message;
    region.classList.add('is-visible');
    timer = setTimeout(() => {
      region.classList.remove('is-visible');
      region.textContent = '';
    }, 2800);
  }, 30);
}
