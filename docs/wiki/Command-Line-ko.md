# 명령줄(`nsql`)

[English](Command-Line) | **한국어**

`nsql`은 Nexa SQL에 함께 들어 있는 명령줄 도구입니다. 앱과 같은 접속 프로필(`nsql conn list`)과 같은 스크립트 엔진을 씁니다. 명령 전체는 `nsql --help`, 명령 하나는 `nsql <명령> -h`로 봅니다.

## 스크립트 파일 없이 질의 실행(`run -q`)

```
nsql run -c <대상> -q "select 1 as a; select 2 as b"
```

`-q <sql>`은 그 글을 스크립트로 실행합니다 — 여러 문장도 되고, 끝에 `;`가 없으면 붙여 줍니다. `-q`가 없으면 `nsql run`은 종전대로 스크립트 파일 경로를 받습니다.

## psql · mysql · sqlcmd에서 쓰던 옵션

접속을 받는 명령(`run` · `export` · `shell` · `conn add`)은 다른 DB CLI의 짧은 옵션도 받으므로 손에 익은 대로 쳐도 됩니다:

| 이렇게 치면 | `nsql`에서 뜻 | 출처 |
|---|---|---|
| `-U <사용자>` | `--user` | psql · sqlcmd |
| `-D <DB>` | `--db` | mysql |
| `-Q <sql>` · `-e <sql>` | `-q`(이 질의 글 실행) | sqlcmd · mysql |
| `-i <파일>` | 스크립트 파일 | sqlcmd |
| `-S 호스트[,포트]`(`tcp:` 접두 허용) | `--host` / `--port` | sqlcmd |
| `-P <숫자>` | `--port` | mysql |
| `-W` | 받고 무시(비밀번호가 필요하면 `nsql`이 묻습니다) | psql |

둘은 위험하거나 뜻이 달라 짧은 안내와 함께 거절합니다(종료 코드 2):

- `-P <비밀번호>`(sqlcmd) — 명령줄의 비밀번호는 셸 기록과 프로세스 목록에 남습니다. `NSQL_PASSWORD` · `--password-stdin`을 쓰거나 `nsql`이 묻게 두세요.
- `-T` — SQL 방언은 `-d <방언>`으로 고릅니다.

`nsql` 자신의 짧은 옵션은 뜻이 그대로입니다: `-d` = 방언 · `-p` = 비밀번호 · `-u` = 사용자 · `-h` = 도움말. `nsql grep`의 `-e`(정규식) · `-i`(대소문자 무시)도 그대로입니다.

`-c`가 없으면 호스트 · 포트 · DB · 사용자 옵션으로 접속하는 모든 명령의 대상을 만듭니다(`nsql run -d mssql -S db.example.com,1433 -U sa -D appdb -Q "select 1"`) — 비밀번호는 `NSQL_PASSWORD`나 질문으로 받고 명령줄에서는 받지 않습니다. `nsql conn add <이름> …`은 같은 옵션을 프로필로 저장합니다. `-c`를 함께 주면 `-c`를 쓰고, 옵션은 쓰이지 않았다는 참고 한 줄이 나옵니다.
