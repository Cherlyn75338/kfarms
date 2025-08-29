/// Overflow and underflow bounds analysis
pub struct BoundsAnalyzer;

impl BoundsAnalyzer {
    /// Calculate maximum safe value for multiplication
    pub fn max_safe_mul(a_max: u128, b_max: u128) -> Option<u128> {
        u128::MAX.checked_div(a_max)?.checked_div(b_max)
    }

    /// Calculate minimum values to avoid underflow
    pub fn min_safe_sub(a: u128, b_max: u128) -> u128 {
        a.saturating_sub(b_max)
    }

    /// Analyze cascading overflow risk
    pub fn analyze_cascade(operations: &[MathOp]) -> Vec<OverflowRisk> {
        let mut risks = Vec::new();
        let mut current_max = 0u128;

        for op in operations {
            match op {
                MathOp::Add(val) => {
                    if let Some(new) = current_max.checked_add(*val) {
                        current_max = new;
                    } else {
                        risks.push(OverflowRisk::Addition(current_max, *val));
                    }
                }
                MathOp::Mul(val) => {
                    if let Some(new) = current_max.checked_mul(*val) {
                        current_max = new;
                    } else {
                        risks.push(OverflowRisk::Multiplication(current_max, *val));
                    }
                }
                MathOp::Div(val) => {
                    if *val == 0 {
                        risks.push(OverflowRisk::DivisionByZero);
                    } else {
                        current_max = current_max / val;
                    }
                }
                MathOp::Sub(val) => {
                    current_max = current_max.saturating_sub(*val);
                }
            }
        }

        risks
    }

    /// Calculate safe operating range
    pub fn safe_range(
        max_value: u128,
        operations: &[MathOp],
        safety_factor: u128,
    ) -> (u128, u128) {
        let min = 0u128;
        let max = max_value / safety_factor;
        (min, max)
    }
}

#[derive(Debug)]
pub enum MathOp {
    Add(u128),
    Sub(u128),
    Mul(u128),
    Div(u128),
}

#[derive(Debug)]
pub enum OverflowRisk {
    Addition(u128, u128),
    Multiplication(u128, u128),
    DivisionByZero,
    Underflow(u128, u128),
}