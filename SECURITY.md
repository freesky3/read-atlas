# Security policy

Read Atlas is currently a Windows Alpha. Security fixes target the latest development version and the latest explicitly supported release; no older release currently has a separate support commitment.

## Report privately

Use this GitHub repository's **Security → Advisories → Report a vulnerability** entry. Include the affected version, a minimal reproduction using synthetic material, and the expected impact.

Do not include live API keys, private PDFs, personal workspace databases, or real conversation content. Do not post exploitable details in a public issue.

If private reporting is not available, open a public issue containing only a request for a private security contact. Wait for a private channel before sharing technical details. Maintainers must enable GitHub private vulnerability reporting when the public repository becomes available, before announcing it.

## Scope

Relevant areas include malicious PDF handling, unsafe Markdown or HTML, Tauri command permissions, workspace path traversal, credential disclosure, cross-workspace data exposure, and unintended repeated provider requests.

Provider account incidents should also be reported to the provider. Rotate an exposed credential immediately rather than waiting for a repository fix.

## 中文

安全漏洞请通过本仓库 **Security → Advisories → Report a vulnerability** 私下报告。若入口不可用，只公开请求一个私下联系方式，不要公开漏洞利用细节、密钥、论文或数据库。维护者应在仓库公开可用后、对外宣传前启用私密漏洞报告。
