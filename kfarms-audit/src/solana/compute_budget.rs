/// Compute budget analyzer
pub struct ComputeBudgetAnalyzer;

impl ComputeBudgetAnalyzer {
    pub const DEFAULT_COMPUTE_UNITS: u64 = 200_000;
    pub const MAX_COMPUTE_UNITS: u64 = 1_400_000;
    
    /// Estimate compute units for operation
    pub fn estimate_compute(operation: &Operation) -> u64 {
        match operation {
            Operation::SimpleTransfer => 5_000,
            Operation::TokenTransfer => 10_000,
            Operation::ComplexCalculation => 50_000,
            Operation::IterativeOperation(n) => 1_000 + (n * 100),
            Operation::CPICall => 20_000,
        }
    }

    /// Check if operation fits in budget
    pub fn fits_in_budget(operations: &[Operation]) -> bool {
        let total: u64 = operations.iter().map(Self::estimate_compute).sum();
        total <= Self::DEFAULT_COMPUTE_UNITS
    }

    /// Identify compute bottlenecks
    pub fn find_bottlenecks(operations: &[Operation]) -> Vec<ComputeBottleneck> {
        let mut bottlenecks = Vec::new();
        
        for (i, op) in operations.iter().enumerate() {
            let compute = Self::estimate_compute(op);
            if compute > Self::DEFAULT_COMPUTE_UNITS / 4 {
                bottlenecks.push(ComputeBottleneck {
                    operation_index: i,
                    estimated_compute: compute,
                    severity: if compute > Self::DEFAULT_COMPUTE_UNITS / 2 {
                        Severity::High
                    } else {
                        Severity::Medium
                    },
                });
            }
        }
        
        bottlenecks
    }
}

#[derive(Debug)]
pub enum Operation {
    SimpleTransfer,
    TokenTransfer,
    ComplexCalculation,
    IterativeOperation(u64),
    CPICall,
}

#[derive(Debug)]
pub struct ComputeBottleneck {
    pub operation_index: usize,
    pub estimated_compute: u64,
    pub severity: Severity,
}

#[derive(Debug)]
pub enum Severity {
    Low,
    Medium,
    High,
}