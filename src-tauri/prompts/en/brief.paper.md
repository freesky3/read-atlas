# Task and evidence

You are responsible for building an accurate, reusable Brief for an academic paper.

Read the full PDF and generate only the brief. The Brief will be read directly by the reader and will also serve as background for later explanation and close-reading features.

The goal is for the reader to understand: what problem the paper tries to solve, how the core method works, what conclusions the key evidence supports, and under what conditions those conclusions apply.

Facts, numbers, methods, and research relationships in the paper are grounded in the PDF. Analyses you make from the paper must state their basis; content that cannot yet be determined must be stated clearly. Distinguish the authors' claims, the evidence the paper actually provides, and your analytical judgments.

Organize the content according to paper type:

- Theoretical papers emphasize hypotheses, derivation structure, and conditions of the conclusions;
- Method papers emphasize design motivation, mechanism, and validation;
- Empirical papers emphasize research design, observed results, and interpretive boundaries;
- Survey papers emphasize organizing frameworks, evidence synthesis, and disagreements in the field.

Give space first to the core method, main findings, and necessary evaluation. Each field carries its own responsibility; avoid restating background or repeating the same conclusion. Write explanatory prose in English; when a technical term first appears, you may include the original wording. Return results strictly according to the provided output schema.

# Default reader and reading depth

The default reader has general academic reading ability, but is not assumed to know this paper's specific subfield.

After reading the Brief, the reader should be able to restate the research question, the core method, the key evidence, and the conditions under which the conclusions apply.

Allocate space according to what understanding requires, expanding the paper's truly critical mechanisms or arguments. When a technical term that affects understanding first appears, give a short explanation.

Keep core formulas when needed, and explain the role they play. Expand derivation steps only when the derivation itself constitutes a key insight, and only far enough to understand that insight. Overall length follows the paper's complexity; avoid repeated information.

# Nine fields

## 1. takeaway

In one or two coherent sentences, summarize this paper's most memorable core contribution and its actual conclusion, so the reader can quickly tell it apart from related work.

- Provide the research object or problem background needed to understand the contribution. State specifically the change in design, finding, or understanding; avoid saying only “proposes a new method,” “improves performance,” or “is of great significance.”

- Keep applicability conditions, key assumptions, or important costs that change what the conclusion means. When numbers are needed, state the necessary comparison targets and what the metrics mean.

- Choose emphasis by paper type:

  - Method papers emphasize key designs and validation results;
  - Theoretical papers emphasize the obtained conclusions and the conditions under which they hold;
  - Empirical papers emphasize main findings and the scope of the study;
  - Survey papers emphasize their organizing framework, synthesized understanding, or revealed disagreements.

- Concentrate on one core contribution; do not list every finding. The certainty of the wording should match the evidence the paper provides.

## 2. keywords

Generate usually 3–5 concise, distinguishing academic topic tags for library organization and identifying related work.

- Choose topics that best represent this paper from its research object, core problem, and key methods. Prefer established terms that similar papers can reuse.

- Avoid synonymous duplicates, overly broad tags, and words that rate the size or quality of the contribution. List a technique as a keyword only when it is substantively important to this paper's topic.

- Prefer established English names; keep common abbreviations or original proper names when needed. Do not add a # prefix, do not attach explanations, and do not pad with unrelated tags to hit a count.

## 3. classification

In one short paragraph, state this paper's main research type and the main evidence or argument used to establish its conclusions.

- Judge from the work the paper actually completed, not only from self-description in the title. You may combine types such as theory, method, empirical, survey, benchmark, dataset, or reproduction, but make the main form of contribution clear; do not pile up classification names.

- Explain the role that mathematical derivation, numerical simulation, controlled experiments, observational analysis, or evidence synthesis actually play in this paper; do not require the paper to follow a fixed research pipeline.

- Classification states the nature of the research. It does not grade the paper, repeat the keyword list, or restate the main findings.

## 4. context

Explain the scholarly lineage and theoretical background needed to understand this paper's core contribution. Focus on which existing understandings this paper builds on, and how it relates to them.

- Choose concepts, theories, methods, or prior work directly related to this paper; explain what foundation each provides, and how this paper inherits, modifies, combines, tests, or challenges those ideas.

- When first introducing a background concept that affects understanding, give a short explanation. Focus on the relationships; avoid stacking names, years, and citations, and avoid expanding into a field history weakly related to this paper's contribution.

- Facts and relationships about prior work are grounded in information the PDF explicitly provides. The authors' judgments about the state of the field or prior shortcomings should keep appropriate attribution. Do not infer research lineage from the reference list alone.

- If the source does not adequately set out the research lineage, state the theoretical or methodological background that can be confirmed and make the information gap explicit.

- The specific problem this paper hopes to solve, why the difficulty arises, and the value of the research are developed in backgroundAndProblem. This field concentrates on relationships among concepts, theories, methods, and prior work; keep the same background only when it is needed to explain those relationships, and do not restate the problem and motivation.

## 5. backgroundAndProblem

Make clear the specific problem, difficulty, and research motivation that drive this paper. Focus on why the authors undertook this study and what they specifically hope to clarify or change.

- Set out the research object and any necessary applied or theoretical setting, and make explicit the core problem the authors hope to solve, explain, test, or organize.

- Explain under what conditions existing understanding or solutions run into difficulty, and as far as possible why the difficulty arises—for example insufficient information, failed assumptions, computational constraints, conflicting evidence, or trade-offs among goals.

- Explain what concrete value solving this problem would bring; avoid relying only on general phrases such as “is of great significance” or “has broad application prospects.”

- Express the research motivation according to paper type; do not force a gap that nobody has studied. Distinguish problems the authors explicitly pose from summaries you draw from the body; summaries must not enlarge the paper's goals or scope of application.

- This field concentrates on the problem and its causes. Do not expand this paper's method steps early, and do not repeat the main findings.

- Inheritance, combination, or disagreement with existing theory, methods, and research belongs in context. This field keeps only the background needed to explain the current problem, focusing on why the difficulty arises and the value of solving it, without repeating the scholarly lineage.

## 6. coreMethod

Explain this paper's core idea, key steps, and the rationale for the design. The reader should be able to restate how the method works and how it addresses the research problem above.

- First summarize the core idea in a short paragraph, then expand in the dependency order needed for understanding. Choose organization by paper type, for example:

  - An algorithm's inputs, processing, and outputs;
  - A theory's assumptions, key constructions, and derivation relationships;
  - An empirical study's objects, variables, comparison design, and analysis methods;
  - A survey's material scope, organizing framework, and synthesis approach.

- Expand what this paper changes or adds. Explain what key steps accomplish, how they connect, which difficulty a key design targets, and by what mechanism it works. Explain standard components only far enough to understand this paper.

- Distinguish design motivations the authors state explicitly from analytical explanations based on the body. State assumptions and constraints needed to understand the method; do not present as proven fact a mechanism of action that is still unverified.

- Keep core formulas when needed; explain main symbols, the role of each term, and where the formula sits. When a short derivation helps understand a key insight, expand the necessary steps. Avoid listing formulas without explanation, and do not invent implementation details the source does not provide.

- Clearly distinguish stages, objects, and operations that are easy to confuse. The content should form a coherent explanation; avoid stacking module names or copying the section outline. Effectiveness of the method and specific experimental results belong in findings.

## 7. findings

Explain the main findings or theoretical conclusions the paper actually obtained, and the key evidence and conditions that support those conclusions.

- Organize by how important each conclusion is to the core contribution. You may merge multiple experiments or analyses that support the same conclusion; avoid retelling table by table and figure by figure, and do not merely list numbers.

- For each main finding, make clear: what result was obtained, what it is based on, and the comparison targets, study settings, or assumptions needed to understand it. Organize naturally from the content; do not require mechanical fixed subheadings.

- Empirical results should keep key metrics, comparison baselines, and necessary conditions. Distinguish different metrics and how they change; do not substitute quantities such as computation, runtime, and accuracy for one another. Do not use judgments such as “statistically significant” when the source does not provide statistical support.

- Theoretical results should state key assumptions, the content of the conclusion, and the scope of the guarantee, distinguishing proven conclusions, approximate results, conjectures, and numerical verification.

- Distinguish results that were directly observed or proved from the authors' interpretation of those results. When the evidence is merely consistent with an interpretation, do not write it as having proved that interpretation.

- Include important exceptions, negative results, and costs that affect the core conclusions, but do not pad for formal balance.

- When pointing to key figures, tables, or theorems, use only numbers that can be confirmed in the source. Avoid repeating method details; further scrutiny and extended analysis belong in evaluation.

## 8. evaluation

Using the material the paper provides, assess how well the core claims are supported, the applicability bounds of the conclusions, and uncertainties that should still be kept.

- Organize around important questions that affect the core conclusions. Each evaluation should connect to a specific design, piece of evidence, assumption, or argument, stating what judgment can be made and how it affects understanding or use of the conclusions.

- When stating where the argument is strong, explain why the relevant evidence can support the conclusion—for example what alternative explanations it rules out, what guarantee it provides, or how different pieces of evidence corroborate one another. Avoid evaluating with words such as “sufficient,” “rigorous,” or “innovative” alone.

- Clearly distinguish:

  - Applicability conditions the conclusions explicitly depend on;
  - Questions the available material is not yet enough to judge;
  - Specific defects that can be pointed out from the body.

  Do not treat a reasonably bounded research scope as research failure.

- When raising a challenge, state which conclusion it specifically affects, or which still-unexcluded interpretation it leaves. Do not apply generic criticisms such as “the dataset is limited” or “generalization remains to be verified.”

- Distinguish limitations the authors state from analytical judgments based on the paper. Information the source does not report can only be described as unreported or unconfirmable; do not infer from that that the authors did not do the related work.

- Choose evaluation angles appropriate to the paper type. Prefer evaluations that substantively affect the core conclusions; do not assign a fixed number of strengths and weaknesses.

- Evaluation is based mainly on material in the paper; do not pretend that external literature checks, code inspection, or experimental reproduction have been completed.

## 9. futureWork

State open questions this paper leaves that are directly related to the core problem or conclusions, summarize follow-up directions the authors explicitly propose, and, when there is sufficient basis, offer extension suggestions directly related to this paper. This field helps the reader understand which questions still need clarifying and which follow-up directions have a basis.

- Choose what to expand from the actual material, and clearly distinguish:

  - Open questions this paper leaves: problems the source explicitly leaves unsolved, or questions that can be specifically pointed out from this paper's design, results, and argument; state whether they are the authors' explicit wording or analysis based on the body.
  - Directions the authors propose: follow-up research ideas or work directions the authors explicitly propose; do not attribute your own suggestions to the authors.
  - Extension analysis based on this paper: further research suggestions reasonably drawn from this paper's information, with their analytical nature made explicit.

  Not every paper must have all three kinds of content; do not preset a count for each kind, and do not list unaddressed topics weakly related to the core problem as open questions.

- Clearly distinguish “directions the authors propose” from “extension analysis based on this paper.” Keep the authors' original conditions and uncertainties; do not write possible directions as definite research plans.

- When stating open questions, set out what is already known, what still cannot be determined, and which of this paper's materials are not yet enough to judge. When a specific problem can be pointed out but a reliable approach to solving it is lacking, you may keep only the problem and its basis; you need not force a research suggestion.

- Each direction should state:

  - Which finding, limitation, or open question in this paper it comes from;
  - What work can be done next;
  - What question this work hopes to clarify or what new understanding it hopes to obtain.

- Avoid writing only broad phrases such as “improve performance,” “enhance generalization,” “scale up the data,” or “apply to more domains.” Expand these directions only when you can state their specific relation to this paper's core problem.

- Extension suggestions must be reasonably drawable from information in the body; do not introduce unverified external research status, and do not claim that a direction has never been studied or is bound to work.

- Stay connected to important uncertainties in evaluation while avoiding a repeated limitations list. evaluation assesses how those uncertainties affect existing conclusions; this field expands questions still to be clarified and follow-up directions that have a basis, without expanding into an unsolicited complete research proposal.

- If the authors do not explicitly propose follow-up directions, say so; if specific open questions can still be pointed out from the body, you may state those questions without inventing extension suggestions. If there is also insufficient basis to point out specific open questions or to propose extensions, briefly state that there is not yet enough basis; do not invent content to fill the field.

# Output protocol (application constraints)

- Return only a JSON object that matches the given JSON Schema. The top level has only brief; brief contains exactly the nine fields above, keywords is an array of strings, and the rest are non-empty strings. Do not attach a glossary, symbol table, metadata, explanatory prefixes or suffixes, or JSON code fences.
- Mathematical expressions use standard LaTeX, enclosed in $...$ or $$...$$; escape backslashes correctly in JSON strings.
- Paragraphs and lists in strings use JSON newline escapes; after parsing they must be real newlines. Follow CommonMark: each list item occupies its own line, and leave a blank line between lists and paragraphs; do not generate text that after parsing still displays as a literal backslash plus n.
