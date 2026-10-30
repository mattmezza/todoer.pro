// Keep mounted task rows and capture forms while applying the server's ordering.
export function reconcileTasks(current, incoming) {
  const sections = new Map(
    [...current.querySelectorAll(':scope > .task-section')].map((node) => [
      node.dataset.sectionId,
      node,
    ]),
  );
  const rows = new Map(
    [...current.querySelectorAll('.task')].map((node) => [node.dataset.id, node]),
  );
  const retained = new Set();
  let sectionCursor = current.firstElementChild;
  for (const next of incoming.querySelectorAll(':scope > .task-section')) {
    const section = sections.get(next.dataset.sectionId);
    if (!section) {
      current.insertBefore(next, sectionCursor);
      continue;
    }
    retained.add(section);
    section.dataset.sectionRevision = next.dataset.sectionRevision;
    section.dataset.sectionTitle = next.dataset.sectionTitle;
    const heading = section.querySelector('.section-heading');
    const nextHeading = next.querySelector('.section-heading');
    if (heading && nextHeading) {
      const collapsed = !heading.querySelector('.section-collapse').open;
      if (!heading.isEqualNode(nextHeading)) {
        nextHeading.querySelector('.section-collapse').open = !collapsed;
        heading.replaceWith(nextHeading);
      }
    }
    const target = section.querySelector('.section-tasks');
    const nextRows = [...next.querySelector('.section-tasks').children];
    let cursor = target.firstElementChild;
    const keep = new Set();
    for (const nextRow of nextRows) {
      const old = rows.get(nextRow.dataset.id);
      const row = old?.isEqualNode(nextRow) ? old : nextRow;
      keep.add(row);
      if (row !== cursor) target.insertBefore(row, cursor);
      cursor = row.nextElementSibling;
    }
    for (const row of [...target.children]) if (!keep.has(row)) row.remove();
    if (sectionCursor !== section) current.insertBefore(section, sectionCursor);
    sectionCursor = section.nextElementSibling;
  }
  for (const section of sections.values()) if (!retained.has(section)) section.remove();
}
