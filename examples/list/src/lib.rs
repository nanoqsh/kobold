use kobold::prelude::*;

#[component]
fn app() -> impl View {
    let count = state!(2_u32);

    view! {
        <div>
            <h1 class="Greeter">"List example"</h1>
            <p>
                "This component dynamically creates a list from a range iterator ending at "
                <button onclick={do *count = count.saturating_sub(1)}>"-"</button>
                " "{ count }" "
                <button onclick={do *count += 1}>"+"</button>
            </p>
            <ul>
            {
                // Use the `for` keyword to turn an iterator into a `View`.
                //
                // On subsequent renders `Kobold` can very cheaply diff items yielded
                // by iterators, avoiding allocations unless new items are added.
                for (1..=count.get()).map(list_item)
            }
    }
}

#[component]
fn list_item(n: u32) -> impl View {
    view! {
        <li>"Item #"{ n }</li>
    }
}

kobold::start!(app);
