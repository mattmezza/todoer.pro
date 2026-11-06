// Native dialogs include their backdrop in pointer events. Only dismiss when
// both the press and release land outside the dialog's content box.
export function outsideDialog(dialog, event) {
  const box = dialog.getBoundingClientRect();
  return (
    event.target === dialog &&
    (event.clientX < box.left ||
      event.clientX > box.right ||
      event.clientY < box.top ||
      event.clientY > box.bottom)
  );
}
