import inflect
import streamlit as st

from dash.client import DashClient
from dash.modules import converter, filter, selector


def draw_page(*, model_name: str):
    # Create engines
    client = DashClient()
    p = inflect.engine()

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

    # Show available actions
    actions = st.tabs(['Whois', 'Boot Management', 'Power Management', ])

    actions[2].selectbox('Current state', ['ON', 'OFF'], disabled=True)

    actions[2].radio('Choose one of options', ['Power ON', 'Power OFF', ])
    action = actions[2].button('Apply')

    # Do specific action
    if action:
        api = k8s.client.BatchV1Api()
        api.create_namespaced_job(
            namespace='kiss',
            name=f'job-power-on-{selected["Name"]}'
        )
        st.success('Successed!')
