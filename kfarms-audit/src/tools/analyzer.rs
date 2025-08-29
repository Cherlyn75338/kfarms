use std::collections::HashMap;

/// Code pattern analyzer
pub struct CodeAnalyzer;

impl CodeAnalyzer {
    /// Find risky patterns in code
    pub fn find_risky_patterns(code: &str) -> Vec<RiskyPattern> {
        let mut patterns = Vec::new();
        
        // Check for unsafe casts
        if code.contains(" as u64") || code.contains(" as u32") {
            patterns.push(RiskyPattern::UnsafeCast);
        }
        
        // Check for unwrap
        if code.contains(".unwrap()") {
            patterns.push(RiskyPattern::UnwrapUsage);
        }
        
        // Check for division
        if code.contains(" / ") && !code.contains("checked_div") {
            patterns.push(RiskyPattern::UncheckedDivision);
        }
        
        patterns
    }

    /// Calculate code complexity
    pub fn calculate_complexity(code: &str) -> ComplexityScore {
        let lines = code.lines().count();
        let branches = code.matches("if ").count() + code.matches("match ").count();
        let loops = code.matches("for ").count() + code.matches("while ").count();
        
        ComplexityScore {
            cyclomatic: branches + loops + 1,
            lines_of_code: lines,
            nesting_depth: Self::max_nesting_depth(code),
        }
    }

    fn max_nesting_depth(code: &str) -> usize {
        let mut max_depth = 0;
        let mut current_depth = 0;
        
        for char in code.chars() {
            match char {
                '{' => {
                    current_depth += 1;
                    max_depth = max_depth.max(current_depth);
                }
                '}' => current_depth = current_depth.saturating_sub(1),
                _ => {}
            }
        }
        
        max_depth
    }
}

#[derive(Debug)]
pub enum RiskyPattern {
    UnsafeCast,
    UnwrapUsage,
    UncheckedDivision,
    UnboundedLoop,
    RecursiveCall,
}

#[derive(Debug)]
pub struct ComplexityScore {
    pub cyclomatic: usize,
    pub lines_of_code: usize,
    pub nesting_depth: usize,
}