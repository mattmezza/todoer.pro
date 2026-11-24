import { onPageLoad } from './page-lifecycle.js';

const selector = '.list-emoji';
const rasterSize = 48;

function centeredEmoji(source, output, emoji, style) {
  const font = `400 ${rasterSize}px ${style.fontFamily}`;
  source.font = font;
  const padding = rasterSize;
  source.canvas.width = Math.ceil(source.measureText(emoji).width) + 2 * padding;
  source.font = font;
  source.fillStyle = style.color;
  source.fillText(emoji, padding, 2 * padding);
  const { width, height } = source.canvas;
  const pixels = source.getImageData(0, 0, width, height).data;
  let left = width;
  let right = -1;
  let top = height;
  let bottom = -1;
  // Font metrics include invisible space. Align the pixels that make up the glyph.
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (pixels[(y * width + x) * 4 + 3] < 24) continue;
      left = Math.min(left, x);
      right = Math.max(right, x);
      top = Math.min(top, y);
      bottom = Math.max(bottom, y);
    }
  }
  if (
    right < left ||
    left === 0 ||
    right === width - 1 ||
    top === 0 ||
    bottom === height - 1
  )
    return null;

  const inkWidth = right - left + 1;
  const inkHeight = bottom - top + 1;
  const scale = rasterSize / Math.max(inkWidth, inkHeight);
  const drawWidth = inkWidth * scale;
  const drawHeight = inkHeight * scale;
  output.clearRect(0, 0, rasterSize, rasterSize);
  output.drawImage(
    source.canvas,
    left,
    top,
    inkWidth,
    inkHeight,
    (rasterSize - drawWidth) / 2,
    (rasterSize - drawHeight) / 2,
    drawWidth,
    drawHeight,
  );
  const image = output.canvas.toDataURL();
  return image.startsWith('data:image/png;') ? `url("${image}")` : null;
}

onPageLoad((scope) => {
  const sourceCanvas = document.createElement('canvas');
  sourceCanvas.height = 3 * rasterSize;
  const source = sourceCanvas.getContext('2d', { willReadFrequently: true });
  const outputCanvas = document.createElement('canvas');
  outputCanvas.width = outputCanvas.height = rasterSize;
  const output = outputCanvas.getContext('2d');
  if (!source || !output) return;

  const images = new Map();
  const rendered = new WeakMap();
  function render(marker) {
    const emoji = marker.textContent;
    if (rendered.get(marker) === emoji) return;
    rendered.set(marker, emoji);
    marker.removeAttribute('data-centered-emoji');
    marker.style.removeProperty('--centered-emoji');
    if (!emoji) return;
    const style = getComputedStyle(marker);
    const key = JSON.stringify([emoji, style.fontFamily, style.color]);
    if (!images.has(key)) {
      try {
        images.set(key, centeredEmoji(source, output, emoji, style));
      } catch {
        // Keep the native text if the browser restricts canvas readback.
        images.set(key, null);
      }
    }
    const image = images.get(key);
    if (!image) return;
    marker.style.setProperty('--centered-emoji', image);
    marker.setAttribute('data-centered-emoji', '');
  }
  function renderWithin(node) {
    if (node.nodeType !== Node.ELEMENT_NODE) return;
    if (node.matches(selector)) render(node);
    for (const marker of node.querySelectorAll(selector)) render(marker);
  }
  renderWithin(document.body);
  const observer = scope.track(
    new MutationObserver((changes) => {
      for (const { target, addedNodes } of changes) {
        const marker = target.nodeType === Node.TEXT_NODE ? target.parentElement : target;
        if (marker?.matches(selector)) render(marker);
        else for (const node of addedNodes) renderWithin(node);
      }
    }),
  );
  observer.observe(document.body, {
    childList: true,
    characterData: true,
    subtree: true,
  });
});
