<img src="lig.png" width="200" align="right" />

# Lig

A SQLite-compatible database library built with Rust.

## Features

* Asynchronous I/O support (_wip_)
* WebAssembly bindings (_wip_)
* SQLite file format compatibility

## Getting Started

Lig is currently read-only so you need a [SQLite database file](https://www.sqlite.org/fileformat.html) for testing.

You can create a databse file with the `sqlite3` program:

```console
$ sqlite3 hello.db
SQLite version 3.42.0 2023-05-16 12:36:15
Enter ".help" for usage hints.
sqlite> CREATE TABLE users (id INT PRIMARY KEY, username TEXT);
sqlite> INSERT INTO users VALUES (1, 'alice');
sqlite> INSERT INTO users VALUES (2, 'bob');
```

You can then start the Lig shell with:

```bash
cargo run hello.db
```

## License

This project is licensed under the [MIT license].

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Lig by you, shall be licensed as MIT, without any additional
terms or conditions.

[MIT license]: https://github.com/penberg/lig/blob/main/LICENSE.md