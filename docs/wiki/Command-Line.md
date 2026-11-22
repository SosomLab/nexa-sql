# Command line (`nsql`)

**English** | [한국어](Command-Line-ko)

`nsql` is the command-line tool that ships with Nexa SQL. It uses the same connection profiles as the app (`nsql conn list`) and the same script engine. Run `nsql --help` for the full list of commands and `nsql <command> -h` for one command.

## Run a query without a script file (`run -q`)

```
nsql run -c <target> -q "select 1 as a; select 2 as b"
```

`-q <sql>` runs the text as a script — several statements are allowed, and a missing `;` at the end is added for you. Without `-q`, `nsql run` takes a script file path as before.

## Options you know from psql, mysql or sqlcmd

Commands that take a connection (`run` · `export` · `shell` · `conn add`) also accept the short options of other database CLIs, so your habits keep working:

| You type | Means in `nsql` | From |
|---|---|---|
| `-U <user>` | `--user` | psql · sqlcmd |
| `-D <db>` | `--db` | mysql |
| `-Q <sql>` · `-e <sql>` | `-q` (run this query text) | sqlcmd · mysql |
| `-i <file>` | the script file | sqlcmd |
| `-S host[,port]` (a `tcp:` prefix is fine) | `--host` / `--port` | sqlcmd |
| `-P <number>` | `--port` | mysql |
| `-W` | accepted and ignored (`nsql` asks for a password when it needs one) | psql |

Two are refused with a short explanation (exit code 2), because they would mean something unsafe or different:

- `-P <password>` (sqlcmd) — a password on the command line ends up in shell history and the process list. Use `NSQL_PASSWORD`, `--password-stdin`, or let `nsql` ask.
- `-T` — use `-d <dialect>` to choose the SQL dialect.

`nsql`'s own short options keep their meaning: `-d` = dialect · `-p` = password · `-u` = user · `-h` = help. `nsql grep` keeps its own `-e` (regex) and `-i` (ignore case).

The host · port · database · user options build a connection when you save a profile (`nsql conn add <name> -d mssql -S db.example.com,1433 -U sa -D appdb`). `run` and `export` still need `-c <target>` (a profile name or a connection string).
