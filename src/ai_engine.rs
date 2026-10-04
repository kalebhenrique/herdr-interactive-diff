use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use crate::git::GitDiff;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComplexityLevel {
    #[serde(alias = "high", alias = "critical")]
    Forte,
    #[serde(alias = "medium", alias = "warn")]
    Media,
    #[serde(alias = "curiosity", alias = "low", alias = "info")]
    Curiosidade,
    Normal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HunkClassification {
    pub hunk_id: String,
    pub level: ComplexityLevel,
    #[serde(default)]
    pub target_line: Option<usize>,
    #[serde(default)]
    pub caveman_msg: Option<String>,
    #[serde(default)]
    pub explanation: Option<String>,
    #[serde(default)]
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClassificationResponse {
    pub classifications: Vec<HunkClassification>,
}

impl ClassificationResponse {
    /// Classificação de demonstração correspondente ao GitDiff::demo()
    pub fn demo() -> Self {
        Self {
            classifications: vec![
                HunkClassification {
                    hunk_id: "src/auth/session.rs#0".to_string(),
                    level: ComplexityLevel::Forte,
                    target_line: Some(19),
                    caveman_msg: Some("PONT NO CHECK! DEREF RAW POINTER DANGEROUS! CRASH CERTO!".to_string()),
                    explanation: Some(
                        "Dereferenciar um ponteiro bruto (*const SessionData) sem garantir alinhamento e validade causa Undefined Behavior (UB). Use Option<&SessionData> ou faça validação defensiva rigorosa antes do bloco unsafe."
                            .to_string(),
                    ),
                    hint: None,
                },
                HunkClassification {
                    hunk_id: "src/storage/cache.rs#0".to_string(),
                    level: ComplexityLevel::Media,
                    target_line: Some(45),
                    caveman_msg: None,
                    explanation: Some(
                        "Uso de unwrap/expect e clone manual desnecessário dentro da seção crítica do Mutex. Pode gerar contenção de concorrência."
                            .to_string(),
                    ),
                    hint: None,
                },
                HunkClassification {
                    hunk_id: "src/compute/pipeline.rs#0".to_string(),
                    level: ComplexityLevel::Curiosidade,
                    target_line: Some(78),
                    caveman_msg: None,
                    explanation: None,
                    hint: Some(
                        "Sabia que filter_map() ou chaining de iteradores evita alocar esse vetor intermediário na heap? Como você reescreveria usando avaliação preguiçosa (lazy)?"
                            .to_string(),
                    ),
                },
                HunkClassification {
                    hunk_id: "src/version.rs#0".to_string(),
                    level: ComplexityLevel::Normal,
                    target_line: None,
                    caveman_msg: None,
                    explanation: None,
                    hint: None,
                },
            ],
        }
    }
}

/// Extrai e limpa blocos de código JSON retornados pelo modelo
pub fn extract_json_payload(raw: &str) -> &str {
    let trimmed = raw.trim();
    if let Some(stripped) = trimmed.strip_prefix("```json") {
        if let Some(end) = stripped.rfind("```") {
            return stripped[..end].trim();
        }
    }
    if let Some(stripped) = trimmed.strip_prefix("```") {
        if let Some(end) = stripped.rfind("```") {
            return stripped[..end].trim();
        }
    }

    // Busca o primeiro '{' e o último '}'
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if start <= end {
            return &trimmed[start..=end];
        }
    }

    trimmed
}

/// Invoca o Antigravity CLI (`agy -p`) em background para classificar o diff
#[allow(dead_code)]
pub async fn classify_diff_with_antigravity(diff_summary: &str) -> Result<ClassificationResponse> {
    let system_instructions = r#"
You are the Herdr Interactive Diff classification engine for Git Diffs.
Analyze each HUNK in the provided diff and classify it into exactly one of the 4 levels:
- "forte": Critical bugs, severe anti-patterns, or major architectural regressions. Provide "caveman_msg" (short, direct, caveman style, no fluff) and "explanation" (detailed technical explanation).
- "media": Attention points that don't break code immediately, but warrant review.
- "curiosidade": Interesting patterns where the user can learn something new. Provide only a "hint" (thought-provoking tip or question, DO NOT provide final answer).
- "normal": Routine, trivial changes.

Return STRICTLY a JSON object matching this schema (no introductory or concluding text):
{
  "classifications": [
    {
      "hunk_id": "hunk_identifier",
      "level": "forte" | "media" | "curiosidade" | "normal",
      "target_line": null,
      "caveman_msg": "...",
      "explanation": "...",
      "hint": "..."
    }
  ]
}
"#;

    let prompt = format!("{}\n\nDiff for analysis:\n{}", system_instructions, diff_summary);

    let output = tokio::process::Command::new("agy")
        .arg("-p")
        .arg(&prompt)
        .output()
        .await
        .context("Failed to invoke Antigravity CLI (agy)")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Antigravity CLI failed with code {:?}: {}", output.status.code(), stderr);
    }

    let stdout_raw = String::from_utf8_lossy(&output.stdout);
    let json_str = extract_json_payload(&stdout_raw);

    let response: ClassificationResponse = serde_json::from_str(json_str)
        .with_context(|| format!("Failed to parse JSON returned by Antigravity CLI:\n{}", stdout_raw))?;

    Ok(response)
}

/// Invokes Antigravity CLI to answer an on-demand question about a specific diff hunk
#[allow(dead_code)]
pub async fn ask_antigravity(hunk_context: &str, user_question: &str) -> Result<String> {
    let prompt = format!(
        "You are Herdr Interactive Diff AI. The developer is reviewing the following diff and has a question:\n\n```diff\n{}\n```\n\nDeveloper question: {}\n\nAnswer concisely, directly, and practically in English:",
        hunk_context, user_question
    );

    let output = tokio::process::Command::new("agy")
        .arg("-p")
        .arg(&prompt)
        .output()
        .await
        .context("Failed to invoke Antigravity CLI for on-demand question")?;

    let answer = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if answer.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Antigravity CLI returned empty response: {}", stderr);
    }

    Ok(answer)
}

/// Builds the professional Pull Request Review prompt covering 5 review lenses in Caveman Style
pub fn build_claude_review_prompt(diff: &str) -> String {
    format!(
r#"You are a senior code reviewer operating in strict Pull Request Review mode (Caveman Style).
Analyze the provided git diff rigorously covering all 5 review lenses:

MANDATORY REVIEW LENSES:
1. security: Vulnerabilities, authorization flaws, data leaks, insecure deserialization, injections, secret exposure, trust boundary violations.
2. readability: Naming, local complexity, confusing control flow, noisy abstractions, code that takes too long to parse.
3. edge_cases: Missing validations, null/empty/None handling, race conditions, failure paths, boundary values, anomalous inputs.
4. maintainability: Coupling, duplication, brittle logic, testability, future change risk, expensive evolution.
5. architecture: Patterns, architectural adherence, responsibility boundaries, layer violations, simpler design opportunities.

CAVEMAN STYLE RULES:
- Extreme brevity. Zero pleasantries. No fluff. No filler words. Telegraphic, high signal-to-noise ratio.
- State problem and exact fix directly.
- Prioritize findings anchored to specific files and lines changed in diff.
- Format for each finding:
  **[HIGH|MEDIUM|LOW] [LENS] [file:line] Direct Title**
  Why: Core reason in 1 sharp sentence.
  Fix: Exact correction and immediate replacement code.

AT THE END OF YOUR RESPONSE, YOU MUST INCLUDE THIS JSON BLOCK TO ANNOTATE THE DIFF:
```json
{{
  "classifications": [
    {{
      "hunk_id": "file_path#0",
      "level": "forte" | "media" | "curiosidade" | "normal",
      "target_line": null,
      "caveman_msg": "SHORT DIRECT CAVEMAN WARNING",
      "explanation": "Detailed technical explanation",
      "hint": "Hint if curiosity level"
    }}
  ]
}}
```

GIT DIFF:
```diff
{}
```
"#,
        diff
    )
}

/// Resolve um identificador de hunk ou nome de arquivo retornado pela IA para um hunk_id válido do GitDiff
pub fn resolve_hunk_id_for_finding(
    hunk_candidate: &str,
    target_line: Option<usize>,
    diff: &GitDiff,
) -> Option<String> {
    // 1. Verifica se já é um hunk_id exato existente no GitDiff
    for file in &diff.files {
        for hunk in &file.hunks {
            if hunk.id == hunk_candidate {
                return Some(hunk.id.clone());
            }
        }
    }

    // 2. Limpa sufixos comuns (#0, :line)
    let clean_candidate = hunk_candidate
        .split('#')
        .next()
        .unwrap_or(hunk_candidate)
        .split(':')
        .next()
        .unwrap_or(hunk_candidate)
        .trim();

    if clean_candidate.is_empty() || clean_candidate == "file_path" || clean_candidate == "unknown" {
        return None;
    }

    // 3. Procura nos arquivos por correspondência de caminho ou basename
    for file in &diff.files {
        let is_match = file.new_path == clean_candidate
            || file.new_path.ends_with(&format!("/{}", clean_candidate))
            || clean_candidate.ends_with(&format!("/{}", file.new_path))
            || std::path::Path::new(&file.new_path)
                .file_name()
                .and_then(|s| s.to_str())
                .map(|b| b == clean_candidate)
                .unwrap_or(false);

        if is_match {
            if let Some(target) = target_line {
                for hunk in &file.hunks {
                    if target >= hunk.new_start && target < hunk.new_start + hunk.new_lines {
                        return Some(hunk.id.clone());
                    }
                }
            }
            if let Some(first_hunk) = file.hunks.first() {
                return Some(first_hunk.id.clone());
            }
        }
    }

    None
}

/// Extrai classificações estruturadas a partir da resposta de review do Claude
/// Suporta tanto bloco JSON (```json ... ```) quanto parsing resiliente das linhas Caveman (**[HIGH] ...**)
pub fn extract_classifications_from_review(review_text: &str, diff: &GitDiff) -> Vec<HunkClassification> {
    let mut results = Vec::new();

    // 1. Tenta extrair via bloco JSON caso presente
    let json_candidate = extract_json_payload(review_text);
    if let Ok(mut resp) = serde_json::from_str::<ClassificationResponse>(json_candidate) {
        if !resp.classifications.is_empty() {
            for c in &mut resp.classifications {
                if let Some(resolved) = resolve_hunk_id_for_finding(&c.hunk_id, c.target_line, diff) {
                    c.hunk_id = resolved;
                }
            }
            return resp.classifications;
        }
    }

    // 2. Parser heurístico resiliente para marcações Caveman:
    // Exemplo: **[HIGH] [security] [src/auth/session.rs:19] TÍTULO**
    //          Why: motivo
    //          Fix: solucao
    let mut current_hunk_id: Option<String> = None;
    let mut current_level = ComplexityLevel::Normal;
    let mut current_title = String::new();
    let mut current_why = String::new();
    let mut current_fix = String::new();

    for line in review_text.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("**[") || trimmed.starts_with("[HIGH]") || trimmed.starts_with("[MEDIUM]") || trimmed.starts_with("[LOW]") {
            // Salva o achado anterior se houver
            if let Some(h_id) = current_hunk_id.take() {
                results.push(HunkClassification {
                    hunk_id: h_id,
                    level: current_level,
                    target_line: None,
                    caveman_msg: Some(current_title.clone()),
                    explanation: Some(format!("Why: {}\nFix: {}", current_why.trim(), current_fix.trim())),
                    hint: if current_level == ComplexityLevel::Curiosidade { Some(current_why.clone()) } else { None },
                });
            }

            current_why.clear();
            current_fix.clear();

            let upper = trimmed.to_uppercase();
            current_level = if upper.contains("HIGH") || upper.contains("FORTE") || upper.contains("CRITICAL") {
                ComplexityLevel::Forte
            } else if upper.contains("MEDIUM") || upper.contains("MEDIA") || upper.contains("WARN") {
                ComplexityLevel::Media
            } else if upper.contains("LOW") || upper.contains("CURIOSIDADE") || upper.contains("INFO") {
                ComplexityLevel::Curiosidade
            } else {
                ComplexityLevel::Normal
            };

            current_title = trimmed.trim_matches('*').trim().to_string();

            // Tenta encontrar o arquivo mencionado na linha (por caminho exato ou basename)
            let mut matched_file = None;
            for file in &diff.files {
                if trimmed.contains(&file.new_path) {
                    matched_file = Some(file);
                    break;
                }
                if let Some(fname) = std::path::Path::new(&file.new_path).file_name().and_then(|s| s.to_str()) {
                    if trimmed.contains(fname) {
                        matched_file = Some(file);
                        break;
                    }
                }
            }

            // Tenta extrair número de linha se presente (ex: [file.rs:42])
            let mut line_no = None;
            if let Some(open) = trimmed.find('[') {
                if let Some(close) = trimmed[open..].find(']') {
                    let bracket_content = &trimmed[open + 1..open + close];
                    if let Some(colon) = bracket_content.rfind(':') {
                        if let Ok(num) = bracket_content[colon + 1..].parse::<usize>() {
                            line_no = Some(num);
                        }
                    }
                }
            }

            if let Some(file) = matched_file {
                if let Some(l) = line_no {
                    for hunk in &file.hunks {
                        if l >= hunk.new_start && l < hunk.new_start + hunk.new_lines {
                            current_hunk_id = Some(hunk.id.clone());
                            break;
                        }
                    }
                }
                if current_hunk_id.is_none() {
                    if let Some(first_hunk) = file.hunks.first() {
                        current_hunk_id = Some(first_hunk.id.clone());
                    }
                }
            }
        } else if trimmed.starts_with("Why:") || trimmed.starts_with("Motivo:") {
            current_why.push_str(trimmed);
            current_why.push('\n');
        } else if trimmed.starts_with("Fix:") || trimmed.starts_with("Correção:") {
            current_fix.push_str(trimmed);
            current_fix.push('\n');
        }
    }

    if let Some(h_id) = current_hunk_id.take() {
        results.push(HunkClassification {
            hunk_id: h_id,
            level: current_level,
            target_line: None,
            caveman_msg: Some(current_title),
            explanation: Some(format!("Why: {}\nFix: {}", current_why.trim(), current_fix.trim())),
            hint: if current_level == ComplexityLevel::Curiosidade { Some(current_why) } else { None },
        });
    }

    results
}

/// Builds the prompt for Antigravity to validate Claude's review against false positives and overengineering
pub fn build_antigravity_validation_prompt(claude_review: &str) -> String {
    format!(
r#"/plan Claude Code generated the following 5-lens technical code review (Security, Readability, Edge Cases, Maintainability, Architecture).
As Antigravity CLI, your mission is to act as a senior anti-overengineering validator and false-positive filter:

1. FALSE-POSITIVE FILTER: Validate whether the risks flagged by Claude are genuine or premature nitpicks/false alarms in context.
2. ANTI-OVERENGINEERING: Identify whether any recommendation adds unnecessary complexity, boilerplate, or speculative abstractions.
3. FINAL CAVEMAN VERDICT: Summarize in a few telegraphic lines strictly what ACTUALLY needs to be fixed before this PR can merge.

CLAUDE REVIEW FOR VALIDATION:
{}
"#,
        claude_review
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_with_fences() {
        let raw = "```json\n{\"classifications\":[]}\n```";
        let extracted = extract_json_payload(raw);
        assert_eq!(extracted, "{\"classifications\":[]}");
    }

    #[test]
    fn test_extract_json_with_surrounding_text() {
        let raw = "Aqui está a análise:\n{\"classifications\":[]}\nEspero que ajude!";
        let extracted = extract_json_payload(raw);
        assert_eq!(extracted, "{\"classifications\":[]}");
    }

    #[test]
    fn test_deserialize_classification_response() {
        let json_data = r#"{
            "classifications": [
                {
                    "hunk_id": "file.rs#0",
                    "level": "forte",
                    "caveman_msg": "BAD PTR!",
                    "explanation": "Raw pointer is not checked"
                },
                {
                    "hunk_id": "file.rs#1",
                    "level": "curiosidade",
                    "hint": "Try using map"
                }
            ]
        }"#;

        let parsed: ClassificationResponse = serde_json::from_str(json_data).unwrap();
        assert_eq!(parsed.classifications.len(), 2);
        assert_eq!(parsed.classifications[0].level, ComplexityLevel::Forte);
        assert_eq!(parsed.classifications[0].caveman_msg.as_deref(), Some("BAD PTR!"));
        assert_eq!(parsed.classifications[1].level, ComplexityLevel::Curiosidade);
    }

    #[test]
    fn test_deserialize_classification_response_with_english_levels() {
        let json_data = r#"{
            "classifications": [
                {
                    "hunk_id": "file.rs#0",
                    "level": "high",
                    "caveman_msg": "CRITICAL FLAW"
                },
                {
                    "hunk_id": "file.rs#1",
                    "level": "medium",
                    "explanation": "Suboptimal lock"
                },
                {
                    "hunk_id": "file.rs#2",
                    "level": "curiosity",
                    "hint": "Use fold instead"
                },
                {
                    "hunk_id": "file.rs#3",
                    "level": "low",
                    "hint": "Minor tip"
                }
            ]
        }"#;

        let parsed: ClassificationResponse = serde_json::from_str(json_data).unwrap();
        assert_eq!(parsed.classifications.len(), 4);
        assert_eq!(parsed.classifications[0].level, ComplexityLevel::Forte);
        assert_eq!(parsed.classifications[1].level, ComplexityLevel::Media);
        assert_eq!(parsed.classifications[2].level, ComplexityLevel::Curiosidade);
        assert_eq!(parsed.classifications[3].level, ComplexityLevel::Curiosidade);
    }

    #[test]
    fn test_build_claude_review_prompt_has_5_lenses() {
        let diff = "+ let x = 1;";
        let prompt = build_claude_review_prompt(diff);

        assert!(prompt.contains("security"));
        assert!(prompt.contains("readability"));
        assert!(prompt.contains("edge_cases"));
        assert!(prompt.contains("maintainability"));
        assert!(prompt.contains("architecture"));
        assert!(prompt.contains("CAVEMAN"));
        assert!(prompt.contains("+ let x = 1;"));

        let val_prompt = build_antigravity_validation_prompt("review text");
        assert!(val_prompt.starts_with("/plan "));
        assert!(val_prompt.contains("ANTI-OVERENGINEERING"));
        assert!(val_prompt.contains("FALSE-POSITIVE"));
    }

    #[test]
    fn test_extract_classifications_from_review_json() {
        let diff = GitDiff::demo();
        let review = r#"
Aqui está minha review:
```json
{
  "classifications": [
    {
      "hunk_id": "src/auth/session.rs#0",
      "level": "forte",
      "caveman_msg": "PERIGO DE MEMORIA!",
      "explanation": "Ponteiro solto"
    }
  ]
}
```
"#;
        let list = extract_classifications_from_review(review, &diff);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].hunk_id, "src/auth/session.rs#0");
        assert_eq!(list[0].level, ComplexityLevel::Forte);
    }

    #[test]
    fn test_extract_classifications_from_review_markdown() {
        let diff = GitDiff::demo();
        let review = r#"
**[HIGH] [security] [src/auth/session.rs:14] DEREF POINTER!**
Why: Falta checar ponteiro nulo.
Fix: Usar Option<&SessionData>.

**[MEDIUM] [readability] [src/storage/cache.rs:45] CLONE EXCESSIVO**
Why: Clone dentro de mutex.
Fix: Mover fora do lock.
"#;
        let list = extract_classifications_from_review(review, &diff);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].level, ComplexityLevel::Forte);
        assert!(list[0].caveman_msg.as_deref().unwrap().contains("DEREF POINTER"));
        assert_eq!(list[1].level, ComplexityLevel::Media);
    }

    #[test]
    fn test_extract_classifications_for_untracked_new_file_matching() {
        let raw_diff = r#"diff --git a/src/components/NewFeature.tsx b/src/components/NewFeature.tsx
new file mode 100644
index 0000000..abcdef1
--- /dev/null
+++ b/src/components/NewFeature.tsx
@@ -0,0 +1,15 @@
+import React from 'react';
+export const NewFeature = () => {
+    return <div>Hello New Feature</div>;
+};
+"#;
        let diff = GitDiff::parse(raw_diff).unwrap();
        assert_eq!(diff.files.len(), 1);
        let expected_hunk_id = &diff.files[0].hunks[0].id;

        // 1. JSON review referencing path without #0
        let review_json = r#"```json
{
  "classifications": [
    {
      "hunk_id": "src/components/NewFeature.tsx",
      "level": "forte",
      "caveman_msg": "MISSING KEY PROPERTY",
      "explanation": "Need key prop"
    }
  ]
}
```"#;
        let parsed_json = extract_classifications_from_review(review_json, &diff);
        assert_eq!(parsed_json.len(), 1);
        assert_eq!(&parsed_json[0].hunk_id, expected_hunk_id);

        // 2. Markdown review referencing basename [NewFeature.tsx:3]
        let review_md = r#"
**[HIGH] [security] [NewFeature.tsx:3] UNVALIDATED PROP**
Why: Prop passed without validation
Fix: Validate prop types
"#;
        let parsed_md = extract_classifications_from_review(review_md, &diff);
        assert_eq!(parsed_md.len(), 1);
        assert_eq!(&parsed_md[0].hunk_id, expected_hunk_id);
    }

    #[test]
    fn test_untracked_single_file_does_not_inherit_unrelated_past_findings() {
        let raw_diff = r#"diff --git a/scripts/setup.sh b/scripts/setup.sh
new file mode 100644
index 0000000..abcdef1
--- /dev/null
+++ b/scripts/setup.sh
@@ -0,0 +1,5 @@
+#!/usr/bin/env bash
+echo "Setup"
+"#;
        let diff = GitDiff::parse(raw_diff).unwrap();
        assert_eq!(diff.files.len(), 1);

        // A past review about a completely different committed file (e.g. src/auth/session.rs)
        let old_review = r#"
**[HIGH] [security] [src/auth/session.rs:14] DEREF POINTER!**
Why: Falta checar ponteiro nulo.
Fix: Usar Option.
"#;
        let parsed = extract_classifications_from_review(old_review, &diff);
        assert!(parsed.is_empty(), "Unrelated past review findings must NOT be attached to unrelated untracked files");
    }
}
