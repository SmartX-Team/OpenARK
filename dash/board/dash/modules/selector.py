import pandas as pd
import streamlit as st
import st_aggrid


def dataframe(df: pd.DataFrame) -> dict:
    '''
    Adds a UI on top of a dataframe to let viewers filter columns

    Args:
        df (pd.DataFrame): Original dataframe

    Returns:
        pd.DataFrame: Filtered dataframe
    '''
    grid_builder = st_aggrid.GridOptionsBuilder.from_dataframe(df)
    grid_builder.configure_column(
        'Name',
        headerCheckboxSelection = True,
    )
    grid_builder.configure_default_column(
        enablePivot=True,
        enableRowGroup=True,
        enableValue=True,
    )
    grid_builder.configure_selection(
        selection_mode='multiple',
        use_checkbox=True,
    )
    grid_builder.configure_side_bar()
    grid_options = grid_builder.build()

    response = st_aggrid.AgGrid(
        df,
        data_return_mode=st_aggrid.DataReturnMode.FILTERED_AND_SORTED,
        enable_enterprise_modules=True,
        fit_columns_on_grid_load=False,
        gridOptions=grid_options,
        header_checkbox_selection_filtered_only=True,
        height=400,
        update_mode=st_aggrid.GridUpdateMode.MODEL_CHANGED,
        use_checkbox=True,
    )
    selected = response['selected_rows']

    if selected is None or not selected:
        return None

    selected = pd.DataFrame(selected)
    del selected['_selectedRowNodeInfo']
    st.write(selected)

    return selected.to_dict(orient='records')
