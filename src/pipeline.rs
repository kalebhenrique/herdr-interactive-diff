#![allow(dead_code)]

use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineStatus {
    Idle,
    Running,
    Completed,
    Failed(String),
}

impl PipelineStatus {
    pub fn label(&self) -> String {
        match self {
            Self::Idle => "Idle (Aguardando)".to_string(),
            Self::Running => "Executando em background...".to_string(),
            Self::Completed => "Concluído com sucesso".to_string(),
            Self::Failed(err) => format!("Erro: {}", err),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ValidationPipeline {
    pub claude_status: PipelineStatus,
    pub claude_output: Option<String>,
    pub antigravity_status: PipelineStatus,
    pub antigravity_output: Option<String>,
}

impl Default for ValidationPipeline {
    fn default() -> Self {
        Self {
            claude_status: PipelineStatus::Idle,
            claude_output: None,
            antigravity_status: PipelineStatus::Idle,
            antigravity_output: None,
        }
    }
}

/// Invoca o Claude Code em background com o diff atual
pub async fn run_claude_review(diff: &str) -> Result<String> {
    let prompt = format!(
        "Você é o Claude Code atuando como revisor de código sênior. Faça um code review rigoroso deste git diff, apontando riscos, bugs potenciais e melhorias arquiteturais de forma concisa e direta:\n\n{}",
        diff
    );

    let output = tokio::process::Command::new("claude")
        .arg("-p")
        .arg(prompt)
        .output()
        .await
        .context("Falha ao invocar Claude Code (claude -p)")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Claude Code retornou erro: {}", stderr);
    }

    let review = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if review.is_empty() {
        anyhow::bail!("Claude Code retornou saída vazia.");
    }

    Ok(review)
}

/// Invoca o Antigravity CLI validando a revisão do Claude contra falsos positivos e overengineering
pub async fn run_antigravity_validation(claude_review: &str) -> Result<String> {
    let prompt = format!(
        "A IA fez uma review, valide falsos positivos e verifique se nao ha over engineering:\n\n{}",
        claude_review
    );

    let output = tokio::process::Command::new("agy")
        .arg("-p")
        .arg(prompt)
        .output()
        .await
        .context("Falha ao invocar Antigravity CLI (agy -p)")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Antigravity CLI retornou erro: {}", stderr);
    }

    let validation = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if validation.is_empty() {
        anyhow::bail!("Antigravity CLI retornou saída vazia.");
    }

    Ok(validation)
}
