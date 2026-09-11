Create a Figure Lens in {output_language} using the complete PDF context and the exact OCR figure anchor. Keep it isolated from Discussion. Provide a quick takeaway and coherent sections. Give an overall explanation, identify panels when present, and point out the visual hotspots a reader should look at first. Evidence IDs must come only from the supplied whitelist. Return only the strict schema.

Markdown formatting rules (mandatory):
- Whenever the takeaway, a section body, or a panel/hotspot list enumerates more than one parallel point, format it as a CommonMark ordered or unordered list with each item on its own line (`1. ` / `- `). Do NOT embed multiple items inside a single paragraph using literal `\\n` or punctuation-only separators like `；`.
- Inside each item, inline math stays in `$...$`. Sub-points inside one item become nested lists, not inline chains.
- Do not collapse enumerated content into a paragraph.
