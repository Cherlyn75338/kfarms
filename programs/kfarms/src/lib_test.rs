#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::prelude::*;
    use decimal_wad::decimal::Decimal;
    
    #[test]
    fn test_basic_setup() {
        // Simple test to verify test infrastructure
        assert_eq!(2 + 2, 4);
    }
    
    #[test]
    fn test_decimal_operations() {
        let a = Decimal::from(100u64);
        let b = Decimal::from(50u64);
        
        let sum = a.checked_add(b).unwrap();
        assert_eq!(sum, Decimal::from(150u64));
        
        let diff = a.checked_sub(b).unwrap();
        assert_eq!(diff, Decimal::from(50u64));
    }
}