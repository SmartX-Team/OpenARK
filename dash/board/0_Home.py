import os
import shutil

from jinja2 import Environment, FileSystemLoader, select_autoescape
import streamlit as st

from dash.client import DashClient


@st.cache_resource()
def load_pages():
    # Load DASH Client
    client = DashClient()

    # Cleanup Pages
    shutil.rmtree('./pages', ignore_errors=True)
    os.mkdir('./pages')

    # Load Page Template
    env = Environment(
        loader=FileSystemLoader(
            searchpath='./templates/',
        ),
        autoescape=select_autoescape(),
    )
    template = env.get_template('model.py.j2')

    # Load Pages
    for index, model in enumerate(client.list_model(), start=1):
        with open(f'./pages/{index:04d}_{model.capitalize()}.py', 'w') as f:
            f.write(template.render(
                model_name=model,
            ))


# Page Information
st.title('Welcome to Noah Cloud Dashboard')

load_pages()
