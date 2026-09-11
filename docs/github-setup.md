# GitHub 仓库设置

本地已准备工作流和模板，但尚未设置远端或发布源码。以下操作需在确定目标 owner/repository 后执行。

## 建立远端和第一次 CI

1. 在目标账号下创建空仓库，先保持 private，避免在检查前意外公开。不要自动创建另一份 README / LICENSE。
2. 将该仓库 URL 设置为 origin，推送经过审查的发布提交。凭据需要允许写入仓库和 GitHub Actions 工作流，不要把 token 写进 remote URL。
3. 在 Actions 中检查 CI 和 Security checks 的真实运行结果。初次失败应修复后再公开。
4. 在 Settings 的分支规则中，为默认分支要求测试与安全检查通过，禁止未经审查的强制推送。保护规则在当前账号／套餐不可用时，应记录替代流程。
5. 启用 Dependabot alerts 与 security updates。仓库已经提供按周检查 npm、Cargo 和 Actions 的配置。

## 公开源码

确认 MIT、第三方声明、密钥扫描及发布文件清单后再改变仓库可见性。角色／素材授权的处理范围由维护者另行决定，MIT 不覆盖第三方权利。

仓库公开后，在 Security 设置里启用 private vulnerability reporting，确认安全报告入口可用，再对外宣传。GitHub 的相关选项会受仓库可见性和账号功能影响；不要把本地 SECURITY.md 的存在当成入口已启用。

本项目 README 和 SECURITY.md 使用相对仓库入口，不需要预先编造 URL 或邮箱。

## 下载包

手动执行 Windows installer 工作流，它只创建 artifact。完成安装、升级、卸载与签名说明后，创建标记为 prerelease 的 Windows Alpha Release，附安装包、SHA256SUMS、支持范围、已知限制和变更说明。

公开 Release 是单独的对外发布动作；成功构建本地安装包不等于已经发布。验收项目见 [发布检查表](release-checklist.md)。
