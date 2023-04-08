import pandas as pd
import streamlit as st


def create_key_map(items: list[object]) -> dict[str, list[str]]:
    # 1. 모든 키(+타입) 취합
    #   - 언더바(_)는 분리기호(/)로 취급
    #   - Object, Array 타입이 섞여있는 경우 skip
    #   - TF-IDF 통해 가치가 높은 단어 (유일한 단어, 별로 등장하지 않는 단어) 선택
    # 2. 중복 데이터 제거
    #   - 우선순위 존재 (spec < status) => 우선해야 할 데이터만 남김
    # 3. 키 간소화
    #   - 키 중복 => 하위 의미까지 구체화
    #       예) /spec/power/address 및 /spec/access/address => Power Address 및 Access Address
    #       예) /spec/power/address 에서 power 외의 자식이 없는 경우 => Power
    #       예) /spec/power/address 및 /spec/cluster => Power (Address 생략!) 및 Cluster
    #       예) /spec/power/address 및 /status/power/address => Power (status 생략!)
    #   - name 등 상징적 심볼 => 상위 의미 고집하기
    if not items:
        return []

    return [
        (renamed.title(), origin.split('/'))
        for renamed, origin in [
            ('name', '/metadata/name/'),
            ('address', '/status/access/primary/address/'),
            ('power', '/spec/power/address/'),
            ('cluster', '/spec/group/cluster_name/'),
            ('role', '/spec/group/role/'),
            ('state', '/status/state/'),
        ]
    ]


@st.cache_data()
def to_dataframe(items: list[object]) -> pd.DataFrame:
    map = create_key_map(items)

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
