use farms::state::*;
use std::mem;

fn main() {
    println!("Actual FarmState size: {}", mem::size_of::<FarmState>());
    println!("Actual UserState size: {}", mem::size_of::<UserState>());
    println!("Expected FarmState: 8336");
    println!("Expected UserState: 920");
    
    println!("FarmState + discriminator: {}", mem::size_of::<FarmState>() + 8);
    println!("UserState + discriminator: {}", mem::size_of::<UserState>() + 8);
}
