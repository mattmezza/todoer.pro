const button = document.querySelector('[data-email-test]');
button?.addEventListener('click', async () => {
  if (button.disabled) return;
  button.disabled = true;
  const notice = document.querySelector('[data-email-notice]');
  notice.textContent = 'Sending test email…';
  try {
    const response = await fetch('/api/admin/email/test', {
      method: 'POST',
      headers: {
        'x-csrf-token': document.querySelector('meta[name="csrf-token"]').content,
      },
    });
    const result = await response.json();
    notice.textContent =
      result.message ||
      result.error?.message ||
      'Unable to send test email. Check configuration and wait 15 minutes before retrying.';
  } catch {
    notice.textContent =
      'Unable to confirm delivery. Check your inbox and wait 15 minutes before retrying.';
  }
});
