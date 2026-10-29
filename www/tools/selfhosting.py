"""Self-hosting offer and expanded practical FAQs."""
repo='https://github.com/mattmezza/todoer.pro'
self_host=section('Your server.<br>Your Todoer.','<div class="self-host-band"><div><span class="pill">FREE TO SELF-HOST</span><h3>Run it yourself. Pay no Todoer subscription.</h3><p>Use your own infrastructure and keep deployment in your hands. You cover hosting, storage and any optional provider costs.</p></div><div><a class="secondary" href="'+repo+'#readme">Get the self-hosted code ↗</a><p class="fine">Setup, updates and backups are your responsibility. See deployment requirements before exposing an instance.</p></div></div>')
title,desc,body=pages['index'];marker=section('Good questions. Clear answers.','').split('<h2>')[0] # add directly before FAQ section
idx=body.index('<section class="wrap block"><h2>Good questions.')
body=body[:idx]+self_host+body[idx:]
body=body.replace('Your tasks, your pace. AI when you want it.','Free core. Cloud or self-hosted. AI when you want it.')
pages['index']=(title,desc,body)
# Self-hosting has the same free software access.
title,desc,body=pages['pricing']
free='<section class="wrap self-host-pricing"><article class="free-plan"><div><span class="pill">THE DIY OPTION</span><h2>Self-hosted</h2><p class="free-price">Free<span> software access</span></p><p>No Todoer subscription. Run the app on infrastructure you control.</p></div><div><ul><li>Your server and storage</li><li>Your deployment, updates and backups</li><li>Optional AI provider usage at your own cost</li></ul><a class="secondary" href="'+repo+'#readme">Start self-hosting ↗</a><p class="fine">Infrastructure is not included. Source use follows the license shipped with the release.</p></div></article></section>'
idx=body.index('<section class="wrap block">');body=body[:idx]+free+body[idx:];pages['pricing']=(title,desc,body)
common=[
('Can I self-host Todoer for free?','Yes. You can run Todoer yourself without paying a Todoer subscription. You provide and pay for your infrastructure, storage and optional AI services, and manage updates, security and backups. Follow the repository deployment instructions and release requirements.'),
('What is the difference between hosted and self-hosted?','The planned hosted service runs the app for you. Self-hosting puts deployment and operations in your hands. Free software access does not include a server, managed backups or a hosted-service support commitment.'),
('Do I need technical experience to self-host?','You should be comfortable running a web service, configuring its environment and authentication, and maintaining persistent storage and backups. If you want to avoid operating a server, the hosted option is intended to provide that convenience.'),
('Can I organize both personal and project tasks?','Yes. Use workspaces and lists to separate areas of your life, sections to group work and nested tasks to break larger outcomes into smaller actions. Start with a single list if that is all you need.'),
('Can I share a list with someone else?','Todoer supports workspace roles and list-level access. Review the audience before sharing or moving work between spaces. Sharing should be a deliberate choice, especially when tasks contain private notes or attachments.'),
('Can I use recurring tasks and reminders?','The app supports recurrence and configured reminders. Email and push delivery depend on the services and device permissions available in your deployment; self-hosted operators need to configure those services.'),
('Does Todoer work offline?','You can read cached recent lists and queue captures for synchronization. This is not full offline editing. Reconnect and confirm pending tasks have synchronized before clearing browser storage or changing devices.'),
('Can I export my work?','Export formats include text, CSV, JSON and ZIP. Choose a format suited to the information you need to preserve. For self-hosted recovery, maintain and test database and attachment backups as well.'),
('Is AI required to use Todoer?','No. Capture, lists, task breakdown and planning work without an AI provider. Optional built-in assistance and external AI-agent access are separate ways to add help when you want it.'),
('Can an AI agent change my tasks automatically?','An authorized compatible agent may use the actions its connection permits. Grant access deliberately and review important writes. Built-in AI task-tree drafts have an explicit review-and-apply flow; external clients follow their own confirmation behavior.'),
('How do I contact you privately?','Email <a href="mailto:info@todoer.pro">info@todoer.pro</a> for product questions, account matters and privacy requests. Never include passwords or access tokens.')]
for key in ['index','pricing']:
 t,d,b=pages[key]
 # Existing questions retain their distinct plan/connector answers; add the practical set below them.
 heading='More about everyday use.' if key=='index' else 'Self-hosting and everyday use.'
 b+=section(heading,faq(common))
 pages[key]=(t,d,b)
