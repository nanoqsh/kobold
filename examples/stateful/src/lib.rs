use kobold::diff::Ver;
use kobold::prelude::*;

struct State {
    name: Ver<String>,
    age: u32,
}

impl State {
    fn new() -> Self {
        State {
            name: Ver::new("Bob"),
            age: 42,
        }
    }
}

#[component]
fn app() -> impl View {
    let state = state!(State::new);

    // Repeatedly clicking the Alice button does not have to do anything.
    let alice = event!(|state| {
        if state.name != "Alice" {
            "Alice".clone_into(&mut state.name);
            Then::Render
        } else {
            Then::Stop
        }
    });

    view! {
        <div>
            // Render can borrow `name` from state, no need for clones
            <h1>{ &state.name }" is "{ state.age }" years old."</h1>
            <button onclick={alice}>"Alice"</button>
            <button onclick={do state.name.push('!')}>"!"</button>
            " "
            <button onclick={do state.age = 18}>"18"</button>
            <button onclick={do state.age += 1}>"+"</button>
    }
}

kobold::start!(app);
