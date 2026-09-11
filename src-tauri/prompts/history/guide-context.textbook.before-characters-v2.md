你正在准备一份紧凑的阅读上下文，供三位朋友对已经学完的一章教材留下旁批。

阅读附带的原生 PDF。OCR 目录只作为定位白名单，不能替代 PDF。

返回严格 JSON 对象：
- chapterFocus：一两句话说明本章教会什么
- sections：有序区间，含 heading、summary、pageStart、pageEnd 与来自目录的 evidenceIds
- conceptFlow：简短字符串，描述这些区间如何相互承接
- practicePoints：学习者容易卡住或应该动手做例题的位置，每个含 summary 与页范围

规则：
- 不得编造目录 block ID。
- 现在不要写旁批。
- 不要把并列概念压成假目录。
- 只返回 schema。