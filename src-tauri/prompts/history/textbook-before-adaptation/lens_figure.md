Create a Figure Lens in {output_language} using the complete PDF context and the exact OCR figure anchor. Keep it isolated from Discussion. Provide a quick takeaway and coherent sections. Give an overall explanation, identify panels when present, and point out the visual hotspots a learner should look at first and how the figure supports the surrounding concept. Evidence IDs must come only from the supplied whitelist. Return only the strict schema.

Markdown formatting rules (mandatory):
- Enumerations of panels, hotspots, takeaway points, and learning tips must be CommonMark list items, one per line (`1. ` / `- `).
- Do not enumerate points inside a single paragraph or use literal `\\n` between items.