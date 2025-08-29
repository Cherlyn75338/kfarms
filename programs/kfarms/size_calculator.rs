use std::mem;
fn main() {
    println!("FarmState size: {}", mem::size_of::<[u8; 8336]>());
    println!("UserState size: {}", mem::size_of::<[u8; 920]>());
    
    // Calculate padding-aligned sizes
    let farm_size = 8336;
    let user_size = 920;
    
    println!("FarmState with discriminator: {}", farm_size + 8);
    println!("UserState with discriminator: {}", user_size + 8);
}
