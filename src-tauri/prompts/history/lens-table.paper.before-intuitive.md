Create a Table Lens in {output_language} using the complete PDF context and the exact OCR table anchor. Keep it isolated from Discussion. Provide a quick takeaway and coherent sections. Explain how rows and columns relate and any calculation a reader must not misread. Evidence IDs must come only from the supplied whitelist. Return only the strict schema.

Markdown formatting rules (mandatory):
- Any enumeration of takeaways, calculations, or reading tips must use a CommonMark list (`1. ` / `- `), one item per line.
- Never write multiple enumerated points inside one paragraph or use literal `\\n` between items. The renderer must see real line breaks before each list marker.
