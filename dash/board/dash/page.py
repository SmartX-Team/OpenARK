import inflect
import streamlit as st

from dash.client import DashClient
from dash.modules import converter, filter, selector


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
    summary = converter.to_dataframe(
        items=items,
        map=[
            ('name', '/metadata/name/'),
            ('address', '/status/access/primary/address/'),
            ('power', '/spec/power/address/'),
            ('cluster', '/spec/group/cluster_name/'),
            ('role', '/spec/group/role/'),
            ('state', '/status/state/'),
        ],
    )

    # Apply filter
    summary = filter.dataframe(summary)

    # Show summary
    st.write(summary)

    # Add selector
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

    selected_item = next(
        item
        for item in items
        if item['metadata']['name'] == selected['Name']
    )

    # Do specific action
    if action:
        client.create_function(
            name=functions[0]['metadata']['name'],
            data=dict(
                box=selected_item,
                power='on' if action_power == 'Power ON' else 'off',
            ),
        )
        st.success('Successed!')
