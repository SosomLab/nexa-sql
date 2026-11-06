use crate::{sql_read_bytes::SqlReadBytes, tds::codec::VarLenContext, ColumnData, VarLenType};

pub(crate) async fn decode<R>(
    src: &mut R,
    ctx: &VarLenContext,
) -> crate::Result<ColumnData<'static>>
where
    R: SqlReadBytes + Unpin,
{
    use VarLenType::*;

    let ty = ctx.r#type();
    let len = ctx.len();
    let collation = ctx.collation();

    let res = match ty {
        Bitn => super::bit::decode(src).await?,
        Intn => super::int::decode(src, len).await?,
        Floatn => super::float::decode(src, len).await?,
        Guid => super::guid::decode(src).await?,
        BigChar | BigVarChar | NChar | NVarchar => {
            ColumnData::String(super::string::decode(src, ty, len, collation).await?)
        }
        Money => {
            let len = src.read_u8().await?;
            super::money::decode(src, len).await?
        }
        Datetimen => {
            let rlen = src.read_u8().await?;
            super::datetimen::decode(src, rlen, len as u8).await?
        }
        #[cfg(feature = "tds73")]
        Daten => super::date::decode(src).await?,
        #[cfg(feature = "tds73")]
        Timen => super::time::decode(src, len).await?,
        #[cfg(feature = "tds73")]
        Datetime2 => super::datetime2::decode(src, len).await?,
        #[cfg(feature = "tds73")]
        DatetimeOffsetn => super::datetimeoffsetn::decode(src, len).await?,
        // ★ nexa-sql patch(11-06): CLR UDT 값 = varbinary(max)와 같은 길(len 0xFFFF = PLP · 아니면 u16 길이) → Binary.
        BigBinary | BigVarBin | Udt => super::binary::decode(src, len).await?,
        Text => super::text::decode(src, collation).await?,
        NText => super::text::decode(src, None).await?,
        Image => super::image::decode(src).await?,
        // ★ nexa-sql patch(11-06): sql_variant = u32 길이(0 = NULL) + 원시 바이트(기저 타입 태그 1 B + 프로퍼티 길이 1 B + 프로퍼티 + 값) → Binary
        //   그대로(기저 타입별 해석은 소비자 몫 · nsql-driver-mssql은 OUT 값을 SQL_VARIANT_PROPERTY로 따로 받는다).
        SSVariant => {
            let n = src.read_u32_le().await? as usize;
            if n == 0 {
                ColumnData::Binary(None)
            } else {
                let mut buf = Vec::with_capacity(n);
                for _ in 0..n {
                    buf.push(src.read_u8().await?);
                }
                ColumnData::Binary(Some(std::borrow::Cow::Owned(buf)))
            }
        }
        t => unimplemented!("{:?}", t),
    };

    Ok(res)
}
