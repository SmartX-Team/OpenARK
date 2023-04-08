import inflect
import streamlit as st

from dash.client import DashClient
from dash.modules import converter, selector


# Create engines
client = DashClient()
p = inflect.engine()


def get_function_title(item: object) -> str:
    metadata = item['metadata']
    annotations = metadata.get('annotations')
    title = annotations.get(
        'dash.ulagbulag.io/title') if annotations else item['metadata']['name']
    return title.title().replace('-', ' ')


def draw_page(*, model_name: str):
    # Page Information
    st.title(p.plural(model_name).title())

    # Get all items
    items = client.list_model_items(
        model_name=model_name,
    )

    # Get boxes summary
    summary = converter.to_dataframe(items)

    # Apply selector
    selected = selector.dataframe(summary)
    if selected is None:
        st.stop()

    # Get all functions
    functions = client.list_function_items(
        model_name=model_name,
    )
    function_names = [
        get_function_title(function)
        for function in functions
    ]

    # Show available actions
    actions = st.tabs(function_names)

    action_power = actions[0].radio('Choose one of options', [
                                    'Power ON', 'Power OFF', ])
    action = actions[0].button('Apply')

    selected_items = (
        item
        for selected_item in selected
        for item in items
        if item['metadata']['name'] == selected_item['Name']
    )

    # Do specific action
    if action:
        for selected_item in selected_items:
            try:
                client.create_function(
                    name=functions[0]['metadata']['name'],
                    data=dict(
                        box=selected_item,
                        power='on' if action_power == 'Power ON' else 'off',
                    ),
                )
            except Exception as e:
                st.error(f'Failed to run "{selected_item["metadata"]["name"]}": {e}')
        st.success('Done!')
