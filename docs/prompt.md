User wants a real-time “second-brain” meeting agent that sits in recurring 1:1s (e.g., with Ben), listens to the conversation, and instantly surfaces context you’ve previously discussed (“3 weeks ago you agreed to do X—close the loop?”). It should also suggest deeper questions in the moment, link to relevant notes/docs/CRM records, and track follow-ups across weeks. Sources include the shared Google Doc agenda/notes, CRM (Salesforce/homegrown), and meeting transcripts. Output: live nudges during the call plus a post-call digest with decisions, open items, and reminders.

Users & context

Primary user: exec/manager in weekly syncs.
Meetings: 1:1s and small working sessions.
Inputs (sources of truth)

Live audio → transcript, speaker tags.
Google Doc (agenda + lightweight notes).
CRM objects (accounts/opps/activities); optionally email/calendar.
Core capabilities

Real-time recall: “On {date} you discussed {topic}; status still open.”
Loop-closure tracker: creates/monitors action items across weeks.
Question prompts: 3–4 smart probes to deepen discussion.
Context fetch: pull relevant bullets from Google Doc/CRM and cite them.
Post-call digest: decisions, open threads, owners/dates; timeline of prior mentions.
Triggers & delivery

Triggers: detected topic/entity (person, deal, project) or “follow-up” language.
Delivery: sidebar/toast during call; DM/email after call; calendar hold if needed.
Success metrics

Recall precision/latency (e.g., <2s to surface).
% follow-ups closed by next meeting.
Reduction in “forgotten” items.
User satisfaction/adoption; false-positive rate.
Risks & mitigations

Privacy/consent: attendee notice + opt-out; redaction of sensitive data.
Hallucinations: cite sources; show exact snippet + link; confidence score.
Data quality: schema mapping & entity resolution across Docs/CRM.
Latency: stream ASR + incremental RAG; cache per-meeting context.
Assumptions

Access to meeting transcripts, Google Doc, and CRM via OAuth/SSO scopes.
User accepts live prompts; post-call summaries are allowed to persist.