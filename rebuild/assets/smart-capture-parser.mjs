// Deterministic suggestions only. The caller must preserve the original input
// until the user has reviewed the interpretation and persisted the metadata.
const boundaries = /[\p{L}\p{N}_-]/u;
const fold = s => s.normalize('NFKC').toLocaleLowerCase();
const dateString = d => `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`;

export function parseCapture(text, { destinations = [], now = new Date(), dates = true } = {}) {
  const spans = [], warnings = [], result = { original: text, title: text, destination: null, tags: [], priority: null, due: null, spans, warnings };
  const exact = (a, b) => fold(a) === fold(b);
  const consume = (start, end, kind, value) => spans.push({ start, end, kind, value, text: text.slice(start,end) });
  const tokens = [];
  for (let i = 0; i < text.length; i++) {
    const symbol = text[i];
    if (!'@#+'.includes(symbol) || (i && !/\s/.test(text[i-1]) && !(symbol === '#' && tokens.at(-1)?.end === i))) continue;
    let end = i + 1, name;
    if (text[end] === '"') {
      const close = text.indexOf('"', end + 1);
      if (close < 0) { warnings.push('Unclosed quoted name'); continue; }
      name = text.slice(end + 1, close); end = close + 1;
    } else {
      while (end < text.length && boundaries.test(text[end])) end++;
      name = text.slice(i+1,end);
    }
    if (name) { tokens.push({ symbol, name, start:i, end }); i=end-1; }
  }
  const spaceTokens = tokens.filter(t=>t.symbol==='@'), listTokens = tokens.filter(t=>t.symbol==='#');
  const space = spaceTokens.length === 1 ? spaceTokens[0] : null;
  const list = listTokens.length === 1 ? listTokens[0] : null;
  if (spaceTokens.length > 1 || listTokens.length > 1) warnings.push('Multiple destinations; choose one');
  const spaceMatches = space ? [...new Set(destinations.filter(d=>exact(d.space,space.name)).map(d=>d.space_id))] : [];
  let matches = list ? destinations.filter(d=>(!space || spaceMatches.length===1 && d.space_id===spaceMatches[0]) && exact(d.list,list.name)) : [];
  if (space && !list && spaceMatches.length === 1) matches = destinations.filter(d=>d.space_id===spaceMatches[0] && d.default_list);
  if ((space || list) && spaceTokens.length <= 1 && listTokens.length <= 1) {
    if (matches.length === 1) {
      result.destination = matches[0];
      if (space) consume(space.start,space.end,'space',matches[0].space);
      if (list) consume(list.start,list.end,'list',matches[0].list);
    } else warnings.push('Destination is unknown or ambiguous');
  }
  for (const t of tokens.filter(t=>t.symbol==='+')) { result.tags.push(t.name); consume(t.start,t.end,'tag',t.name); }
  const priorities = [...text.matchAll(/(?:^|\s)(p[0-3]|![0-3]|!{1,3})(?=\s|$)/gi)];
  if (priorities.length === 1) {
    const m = priorities[0], token = m[1].toLowerCase();
    const value = /^!+$/.test(token) ? 3-token.length : Number(token[1]);
    result.priority = value; consume(m.index + m[0].length-token.length,m.index + m[0].length,'priority',value);
  } else if (priorities.length > 1) warnings.push('Multiple priorities; choose one');
  if (dates) {
    const found = [...text.matchAll(/(?:^|\s)(today|tomorrow|in\s+\d+\s+days?|(?:next\s+)?(?:monday|tuesday|wednesday|thursday|friday|saturday|sunday))(?=\s|$|[,.!?])/gi)];
    if (found.length === 1) {
      const m = found[0], phrase = m[1].toLowerCase(), d = new Date(now.getFullYear(),now.getMonth(),now.getDate());
      if (phrase === 'tomorrow') d.setDate(d.getDate()+1);
      else if (phrase.startsWith('in ')) { const n = Number(phrase.match(/\d+/)[0]); if (n <= 3650) d.setDate(d.getDate()+n); else warnings.push('Date is too far away'); }
      else if (phrase !== 'today') {
        const target = ['sunday','monday','tuesday','wednesday','thursday','friday','saturday'].indexOf(phrase.replace('next ',''));
        let offset = (target-d.getDay()+7)%7 || 7;
        if (phrase.startsWith('next ') && target > d.getDay()) offset+=7;
        d.setDate(d.getDate()+offset);
      }
      if (!warnings.includes('Date is too far away')) { result.due=dateString(d); consume(m.index+m[0].length-m[1].length,m.index+m[0].length,'date',result.due); }
    } else if (found.length > 1) warnings.push('Multiple dates; choose one');
  }
  let title = text;
  for (const span of [...spans].sort((a,b)=>b.start-a.start)) title=title.slice(0,span.start)+title.slice(span.end);
  result.title = title.replace(/\s+/g,' ').trim();
  if (!result.title) { result.title=text; result.destination=null; result.tags=[]; result.priority=null; result.due=null; spans.length=0; warnings.push('Task title cannot be empty'); }
  return result;
}
