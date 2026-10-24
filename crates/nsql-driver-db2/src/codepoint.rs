//! DRDA/DDM 코드포인트(Open Group DRDA V5 · Apache Derby `CodePoint.java`와 같은 값 · 1차에 쓰는 것만).
//!
//! 값은 docs/121b(협업 조사)로 대조한다 — 틀린 것이 있으면 여기 한 곳만 고친다.
#![allow(dead_code)]

// ── 명령(RQSDSS)
pub const EXCSAT: u16 = 0x1041;
pub const ACCSEC: u16 = 0x106D;
pub const SECCHK: u16 = 0x106E;
pub const ACCRDB: u16 = 0x2001;
pub const CLSQRY: u16 = 0x2005;
pub const CNTQRY: u16 = 0x2006;
pub const EXCSQLSTT: u16 = 0x2009;
pub const EXCSQLIMM: u16 = 0x200A;
pub const OPNQRY: u16 = 0x200C;
pub const PRPSQLSTT: u16 = 0x200D;
pub const RDBCMM: u16 = 0x200E;
pub const RDBRLLBCK: u16 = 0x200F;
pub const DSCSQLSTT: u16 = 0x2008;
pub const EXCSQLSET: u16 = 0x2014;

// ── 응답(RPYDSS)
pub const EXCSATRD: u16 = 0x1443;
pub const ACCSECRD: u16 = 0x14AC;
pub const SECCHKRM: u16 = 0x1219;
pub const ACCRDBRM: u16 = 0x2201;
pub const OPNQRYRM: u16 = 0x2205;
pub const ENDQRYRM: u16 = 0x220B;
pub const ENDUOWRM: u16 = 0x220C;
pub const RDBNFNRM: u16 = 0x2211;
pub const RDBAFLRM: u16 = 0x221A;
pub const RDBATHRM: u16 = 0x22CB;
pub const CMDCHKRM: u16 = 0x1254;
pub const PRCCNVRM: u16 = 0x1245;
pub const SYNTAXRM: u16 = 0x124C;
pub const MGRLVLRM: u16 = 0x1210;
pub const AGNPRMRM: u16 = 0x1232;
pub const ABNUOWRM: u16 = 0x220D;
pub const SQLERRRM: u16 = 0x2213;
pub const OPNQFLRM: u16 = 0x2212;
pub const QRYNOPRM: u16 = 0x2202;
pub const QRYPOPRM: u16 = 0x220F;
pub const RDBUPDRM: u16 = 0x2218;
pub const CMDCMPRM: u16 = 0x124B;
pub const DSCINVRM: u16 = 0x220A;
pub const RSCLMTRM: u16 = 0x1233;

// ── 객체(OBJDSS 또는 명령 안)
pub const SQLCARD: u16 = 0x2408;
pub const SQLDARD: u16 = 0x2411;
pub const SQLDTA: u16 = 0x2412;
pub const SQLDTARD: u16 = 0x2413;
pub const SQLSTT: u16 = 0x2414;
pub const SQLATTR: u16 = 0x2450;
pub const QRYDSC: u16 = 0x241A;
pub const QRYDTA: u16 = 0x241B;
pub const EXTDTA: u16 = 0x146C;
pub const TYPDEFNAM: u16 = 0x002F;
pub const TYPDEFOVR: u16 = 0x0035;
pub const CCSIDSBC: u16 = 0x119C;
pub const CCSIDDBC: u16 = 0x119D;
pub const CCSIDMBC: u16 = 0x119E;

// ── 매개변수
pub const EXTNAM: u16 = 0x115E;
pub const SRVNAM: u16 = 0x116D;
pub const SRVRLSLV: u16 = 0x115A;
pub const SRVCLSNM: u16 = 0x1147;
pub const MGRLVLLS: u16 = 0x1404;
pub const AGENT: u16 = 0x1403;
pub const SQLAM: u16 = 0x2407;
pub const RDB: u16 = 0x240F;
pub const SECMGR: u16 = 0x1440;
pub const CMNTCPIP: u16 = 0x1474;
pub const SECMEC: u16 = 0x11A2;
pub const SECCHKCD: u16 = 0x11A4;
pub const SECTKN: u16 = 0x11DC;
pub const USRID: u16 = 0x11A0;
pub const PASSWORD: u16 = 0x11A1;
pub const RDBNAM: u16 = 0x2110;
pub const RDBACCCL: u16 = 0x210F;
pub const PRDID: u16 = 0x112E;
pub const PRDDTA: u16 = 0x2104;
pub const PKGNAMCSN: u16 = 0x2113;
pub const PKGSN: u16 = 0x2112;
pub const PKGSNLID: u16 = 0x2124;
pub const RDBCMTOK: u16 = 0x2105;
pub const CRRTKN: u16 = 0x2135;
pub const QRYBLKSZ: u16 = 0x2114;
pub const MAXBLKEXT: u16 = 0x2141;
pub const QRYROWSET: u16 = 0x2156;
pub const QRYINSID: u16 = 0x215B;
pub const QRYPRCTYP: u16 = 0x2102;
pub const QRYBLKCTL: u16 = 0x2132;
pub const QRYCLSIMP: u16 = 0x215D;
pub const QRYATTSCR: u16 = 0x2149;
pub const QRYATTSNS: u16 = 0x2157;
pub const QRYATTUPD: u16 = 0x2150;
pub const SVRCOD: u16 = 0x1149;
pub const SYNERRCD: u16 = 0x114A;
pub const CODPNT: u16 = 0x000C;
pub const RDBINTTKN: u16 = 0x2103;
pub const RDBALWUPD: u16 = 0x211A;
pub const STTSTRDEL: u16 = 0x2120;
pub const STTDECDEL: u16 = 0x2121;
pub const MAXRSLCNT: u16 = 0x2140;
pub const RSLSETFLG: u16 = 0x2142;
pub const TRGNSPNM: u16 = 0x1161;
pub const UOWDSP: u16 = 0x2115;
pub const RTNSQLDA: u16 = 0x2116;
pub const NBRROW: u16 = 0x213A;
pub const OUTEXP: u16 = 0x2111;
pub const SQLCSRHLD: u16 = 0x211F;
pub const DIAGLVL: u16 = 0x2160;

// ── 보안 메커니즘(SECMEC 값)
pub const SECMEC_USRIDPWD: u16 = 3;
pub const SECMEC_USRIDONL: u16 = 4;
pub const SECMEC_USRENCPWD: u16 = 7;
pub const SECMEC_USRSSBPWD: u16 = 8;
pub const SECMEC_EUSRIDPWD: u16 = 9;

// ── 매니저 레벨(MGRLVLLS 값 · Derby 클라이언트가 보내는 값)
pub const MGRLVL_AGENT: u16 = 7;
pub const MGRLVL_SQLAM: u16 = 7;
pub const MGRLVL_RDB: u16 = 7;
pub const MGRLVL_SECMGR: u16 = 7;
pub const MGRLVL_CMNTCPIP: u16 = 5;
pub const MGRLVL_UNICODEMGR: u16 = 1208;
pub const UNICODEMGR: u16 = 0x1C08;
