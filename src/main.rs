mod executer;

fn main() {
    let executer = executer::Executer::new();

    executer.spawn(async {
        println!("Hello world");
        async {
            println!("Hello world 2");
            async { println!("Hello world 2.1") }.await;
        }
        .await;
        async { println!("Hello world 3") }.await;
    });

    executer.spawn(async {
        println!("Hello world");
    });

    executer.spawn(async {
        println!("Hello world");
    });

    executer.run();
}
