# Пример реализации async executer, который работает в текущем потоке исполнения

Вот пример кода:

```rust
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
```

```sh
$ cargo run

Hello world
Hello world 2
Hello world 2.1
Hello world 3
Hello world
Hello world
```
