Create a Table Lens in {output_language} using the complete PDF context and the exact OCR table anchor. Keep it isolated from Discussion. Provide a quick takeaway and coherent sections. Explain how rows and columns relate and how to read the table to answer a study question. Evidence IDs must come only from the supplied whitelist. Return only the strict schema.

Markdown formatting rules (mandatory):
- Use CommonMark list items for any enumeration (column meanings, calculation walk-through, takeaway points).
- One item per line; never chain items with `；` or literal `\\n`.