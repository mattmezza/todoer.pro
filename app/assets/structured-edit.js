import { parseCapture } from './smart-capture-parser.mjs';
// Every task editor applies capture notation in the same atomic save as its brief.
export function mountStructuredEdit(_input) {
  return async (api, id, change) => {
    const parsed = parseCapture(change.title);
    if (parsed.destination || parsed.section_id || /(^|\s)[@#$]/u.test(change.title))
      throw Error(
        'Use Move to change a destination; remove destination notation before saving.',
      );
    if (parsed.warnings.length) throw Error(parsed.warnings.join('. '));
    if (!parsed.spans.length) return api(`/tasks/${id}`, 'PATCH', change);
    if (!parsed.title.trim()) throw Error('Enter a task title as well as notation.');
    const metadata = {
      priority: parsed.priority,
      due_date: parsed.due,
      planned_date: parsed.planned,
      duration_minutes: parsed.duration,
      tags: parsed.tags,
    };
    return api(`/tasks/${id}/structured-edit`, 'POST', {
      ...change,
      title: parsed.title.trim(),
      metadata,
    });
  };
}
