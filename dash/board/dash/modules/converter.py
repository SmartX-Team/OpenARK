import pandas as pd
import streamlit as st


@st.cache_data()
def to_dataframe(*, items: list[object], map: list[tuple[str, str]]) -> pd.DataFrame:
    map = [
        (renamed.title(), origin.split('/'))
        for renamed, origin in map
    ]

    def get_jq_style(data, keys: list[str]):
        if not keys:
            return data
        if not keys[0]:
            return get_jq_style(data, keys[1:])
        if data is None or keys[0] not in data:
            return None
        return get_jq_style(data[keys[0]], keys[1:])

    return pd.DataFrame.from_dict({
        renamed: [
            get_jq_style(item, origin)
            for item in items
        ]
        for renamed, origin in map
    })
