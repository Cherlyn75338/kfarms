use std::collections::HashMap;

/// Audit report generator
pub struct ReportGenerator;

impl ReportGenerator {
    /// Generate summary statistics
    pub fn generate_summary(findings: &[Finding]) -> Summary {
        let mut severity_counts = HashMap::new();
        
        for finding in findings {
            *severity_counts.entry(finding.severity.clone()).or_insert(0) += 1;
        }
        
        Summary {
            total_findings: findings.len(),
            critical: severity_counts.get("Critical").copied().unwrap_or(0),
            high: severity_counts.get("High").copied().unwrap_or(0),
            medium: severity_counts.get("Medium").copied().unwrap_or(0),
            low: severity_counts.get("Low").copied().unwrap_or(0),
        }
    }

    /// Format finding for report
    pub fn format_finding(finding: &Finding) -> String {
        format!(
            "## {} - {}\n\n**Description:** {}\n\n**Impact:** {}\n\n**Recommendation:** {}\n",
            finding.severity,
            finding.title,
            finding.description,
            finding.impact,
            finding.recommendation
        )
    }
}

#[derive(Debug)]
pub struct Finding {
    pub severity: String,
    pub title: String,
    pub description: String,
    pub impact: String,
    pub recommendation: String,
}

#[derive(Debug)]
pub struct Summary {
    pub total_findings: usize,
    pub critical: usize,
    pub high: usize,
    pub medium: usize,
    pub low: usize,
}