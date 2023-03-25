from pandas.api.types import (
    is_categorical_dtype,
    is_datetime64_any_dtype,
    is_numeric_dtype,
    is_object_dtype,
)
import pandas as pd
import streamlit as st


def dataframe(df: pd.DataFrame) -> dict:
    '''
    Adds a UI on top of a dataframe to let viewers filter columns

    Args:
        df (pd.DataFrame): Original dataframe

    Returns:
        pd.DataFrame: Filtered dataframe
    '''
    selected = st.selectbox(
        label='Choose one of',
        options=df[['Name']],
    )
    if selected is None:
        return None

    return df.loc[df['Name'] == selected].to_dict(orient='records')[0]
