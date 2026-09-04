You are given one screenshot of a mock HTML UI that a reviewer is about to comment on. Describe factually what is on screen so a coding agent later knows which view and which elements the comment refers to.

Rules:
- No opinions, no suggestions, no guesses about intent.
- Ignore the desktop bar, notifications, or anything at the screen edges that is not part of the page.
- `title`: a short name for the view (3–8 words), e.g. "Settings page, Billing tab".
- `summary`: 2–3 sentences: what kind of page, its main layout, and its apparent state (empty, loading, modal open, form half-filled…).
- `regions`: top-to-bottom, one entry per distinct area (header, sidebar, main list, footer, modal…). `contents` is at most ~40 words: quote headings, labels, button text, and key values exactly; do not transcribe body paragraphs.
- `notable`: only things a reviewer would plausibly point at: misalignment, clipped or overflowing text, inconsistent spacing or styling, placeholder or lorem text, broken images, empty states. Leave the list empty if nothing stands out.
Fill every schema field.
