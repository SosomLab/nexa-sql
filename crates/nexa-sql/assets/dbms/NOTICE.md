# DBMS 아이콘 — 출처 · 라이선스 · 상표 고지(10-04 · 사용자 결정 = 173종 전부 포함 + 이 고지)

> 이 폴더의 PNG 173종 × 2테마(light/dark) × 2크기(64/20)는 **접속 대상 제품을 화면에 표시하기 위한 용도**로만 쓴다(객체 탐색기 서버 머리글 · 연결 행).
> 원본 작업 폴더 = 개발자 PC의 `dbms-icons/`(수집 원천 `sources/<id>/` · 받은 URL 기록 `sources/fetch-log.json` · 제품표 `manifest.json` · 재생성 도구 `tools/`). 이 저장소에는 결과 PNG만 넣는다.
> 사용자 결정(10-04): **173종 전부 포함 + 이 고지**. 아래 §3은 후속 확인 과제다. 권리자가 요청하면 해당 그림은 자체 구성형(대표색 타일 + DB 원통 + 제품명/약어)으로 바꾼다.

## 1. 상표

여기 쓰인 제품 이름·로고는 **각 소유 회사의 상표 또는 등록 상표**다. Nexa SQL은 해당 회사와 제휴하거나 그 회사의 보증을 받지 않았다. 로고는 사용자가 접속한 제품이 무엇인지 알려 주는 **지명적 사용**(nominative use)으로만 쓰며, 마케팅 · 스토어 이미지 · 제품 아이콘으로 쓰지 않는다.

## 2. 원천별 라이선스(큰 이미지 기준 · 173종)

| 원천 | 수 | 파일 라이선스 | 필요한 고지 | 해당 id |
|---|---|---|---|---|
| devicon | 27 | MIT | MIT 저작권·허가 문구를 배포물에 포함 | mysql sqlserver postgresql mariadb firebird supabase vitess yugabytedb clickhouse sparksql sqlite rocksdb realm redis memcached mongodb couchbase couchdb cosmosdb firestore dynamodb consul cassandra neo4j elasticsearch prometheus firebase |
| simple-icons | 19 (+ 작은 이미지 흰 마크 33) | CC0 1.0 | 없음(상표 고지는 §1) | saphana sqlanywhere maxdb interbase gaussdb polardb spanner bigquery cloudsql doris druid crate filemaker parquet etcd bigtable opensearch splunk fauna |
| Wikimedia Commons | 20 | **파일마다 다름**(대부분 PD-textlogo · 상표 표시 조건) | 파일 페이지 확인 · 필요하면 저작자 표시 | sybase teradata informix ingres vertica netezza tibero altibase redshift azuresql pinot kylin hive access berkeleydb ignite riak hbase accumulo solr |
| GitHub 조직 아바타 | 73 | **명시 없음**(각 조직의 저작물·상표) | — | `manifest.json`에서 `large.source`가 `https://github.com/…`인 항목 |
| 공식 사이트 이미지·파비콘 | 14 | **명시 없음**(각 회사 저작물·상표) | — | dameng gbase actianzen synapse fabric alloydb sqlce dbase foxpro keydb geode extremedb kvrocks nebulagraph |
| 자체 구성(대표색 타일 + DB 원통 + 제품명 글자) | 20 | 자체 제작 | 없음(글자는 제품명 = 상표 · §1) | oracle db2 kingbase aurora rds impala hsqldb derby leveldb csv garnet timesten documentdb keyspaces neptune iris cache odbc jdbc generic |

- 작은 이미지(20 px): 90종 = 대표색 타일 + 약어(자체 제작) · 50종 = 큰 이미지 축소 · 33종 = simple-icons 흰 마크(CC0).
- 구성형 글자는 macOS 글꼴(Arial Black/Bold/Narrow Bold)로 래스터화해 이미지에 들어 있다(글꼴 파일 배포 아님).
- 회사 로고 + 제품명 캡션형 중 회사 로고가 simple-icons(CC0)가 아닌 것: Microsoft 파비콘 4(synapse · fabric · sqlce · foxpro) · Google 그라데이션 1(alloydb) · Actian 로고 1(actianzen) · GitHub 아바타 2(KxSystems · marklogic) · devicon 1(firestore).

### 2-1. devicon MIT 고지(배포물 포함용 · 원문 확인 필요)

```text
devicon — https://github.com/devicons/devicon
MIT License · Copyright (c) 2015 konpa(및 기여자 — 원문 LICENSE의 저작권 줄을 그대로 옮길 것)
Permission is hereby granted, free of charge, … (MIT 전문)
```

## 3. 후속 확인 과제

1. **GitHub 조직 아바타 73종** — 라이선스 표시가 없는 각 조직의 저작물. 지명적 사용은 업계 관행(DBeaver · DataGrip 등)이지만 재배포 허락은 아니다. 가장 큰 덩어리.
2. **공식 사이트 이미지·파비콘 14종** — 위와 같음. 그중 Microsoft 회사 로고 캡션형 4종은 Microsoft 브랜드 지침(로고 변형 · 결합 금지) 저촉 가능성.
3. **Wikimedia Commons 20종** — Commons는 자유 파일만 받지만 로고는 대개 "PD-textlogo + 상표" 조건 · 파일별 라이선스를 `fetch-log.json`의 `wm` URL로 하나씩 확인해야 한다.
4. **로고 변형** — 다크 테마 명도 반전 · 흰 타일 받침 · 잘라 냄은 일부 회사 브랜드 지침이 금지한다(Oracle · Microsoft · IBM · SAP · AWS · Google 계열 지침이 엄격 — 원천 README 권고). 이들 대부분은 구성형·simple-icons 마크라 위험은 낮다.
5. **devicon MIT** — 허용되지만 §2-1 고지를 배포물(앱 "정보" 또는 설치본의 제3자 고지 파일)에 넣어야 한다 — 지금은 저장소의 이 파일까지(설치본 반영 = 후속 · [33](../../../../docs/33-distribution-and-packaging.md)) · §2-1의 원문 저작권 줄 확인.


- 권리자 요청 시: 해당 id의 그림을 자체 구성형으로 바꾸고(`~/Desktop/dbms-icons/tools/build.py`의 `L` 표 → 재생성 → `assets/dbms/{light,dark}/{64,20}/<id>.png` 교체) 이 표를 고친다.
