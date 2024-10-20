use kobold::prelude::*;

#[component]
fn hello(name: &str) -> impl View + '_ {
    view! {
        // No need to close tags at the end of the macro
        <h1> "Hello "{ name }"!"
    }
}

fn main() {
    kobold::start(view! {
        <!hello name="Kobold">
    });
}

mod talk {


    #[derive(Clone, Copy)]
    #[repr(u8)]
    enum Byte {
        Letter,
        Digit,
        Punct,
    }

    #[derive(Clone, Copy)]
    struct Error;

    fn parse(byte: u8) -> Result<Byte, Error> {
        const LUT: [Result<Byte, Error>; 256] = {
            let mut table = [Err(Error); 256];

            table[b'a' as usize] = Ok(Byte::Letter);
            // ...
            table[b'.' as usize] = Ok(Byte::Punct);

            table
        };

        LUT[byte as usize]
    }



    fn is_alphanumeric(byte: Byte) -> bool {
        const LUT: [bool; 3] = {
            let mut table = [false; 3];

            table[Byte::Letter as usize] = true;
            table[Byte::Digit as usize] = true;

            table
        };

        LUT[byte as usize]
    }




}


