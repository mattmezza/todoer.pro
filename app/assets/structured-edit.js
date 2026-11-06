import { parseCapture } from './smart-capture-parser.mjs';
import { confirmAction } from './confirm.js';
// Every task editor uses the same notation parser as capture. Interpretation is
// reviewed before saving so title and metadata change together.
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
    const details = [
      `Title: ${parsed.title}`,
      parsed.priority !== null && `Priority: ${parsed.priority}`,
      parsed.due && `Due: ${parsed.due}`,
      parsed.planned && `Planned: ${parsed.planned}`,
      parsed.duration !== null && `Duration: ${parsed.duration} minutes`,
      parsed.tags.length && `Add tags: ${parsed.tags.join(', ')}`,
      'Other task metadata will be preserved.',
    ]
      .filter(Boolean)
      .join('\n');
    if (
      !(await confirmAction(details, {
        title: 'Review task notation',
        acceptLabel: 'Save interpretation',
      }))
    )
      return null;
    return api(`/tasks/${id}/structured-edit`, 'POST', {
      ...change,
      title: parsed.title.trim(),
      metadata,
    });
  };
}
