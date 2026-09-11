Create a Formula Lens in {output_language} using the complete PDF context and the exact OCR formula anchor. Keep it isolated from Discussion. Provide a quick takeaway and coherent sections. Include every non-trivial symbol with provenance. Reconstruct the LaTeX and explain what the formula does and where to start. Evidence IDs must come only from the supplied whitelist. Return only the strict schema.

Markdown formatting rules (mandatory):
- Use proper CommonMark lists whenever content enumerates multiple parallel items (numbered findings, ordered steps, bullet takeaways, hotspot highlights). Every numbered item must start on its own line beginning with `1. `, `2. `, ... — never glue items together with literal `\\n1.` or `；` separators.
- Inside a single list item, the inline math must still be `$...$`. Multiple sub-points inside one item use a nested bullet or a sub-numbered list, not `；` chains.
- Headings, code, and tables are not required. Lists and inline math are.
