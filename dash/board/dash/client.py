import os

import requests
import streamlit as st


class DashClient:
    def __new__(cls) -> 'DashClient':
        @st.cache_resource()
        def init() -> 'DashClient':
            client = object.__new__(cls)
            client.__init__()
            return client

        return init()

    def __init__(self) -> None:
        self._session = requests.Session()
        self._host = os.environ.get('DASH_HOST') or 'http://localhost:9999'

    def _call_raw(self, *, method: str, path: str, data: object = None) -> object:
        response = self._session.request(
            method=method,
            url=f'{self._host}{path}',
            json=data,
        )

        data = response.json() if response.text else {}
        if response.status_code == 200:
            if 'spec' in data:
                return data['spec']
            raise Exception(f'Failed to execute {path}: no output')
        if 'spec' in data:
            raise Exception(f'Failed to execute {path}: {data["spec"]}')
        raise Exception(
            f'Failed to execute {path}: status code [{response.status_code}]')

    def list_model(self) -> list[str]:
        return self._call_raw(
            method='GET',
            path=f'/model/',
        )

    def list_model_items(self, *, model_name: str) -> list[object]:
        return self._call_raw(
            method='GET',
            path=f'/model/{model_name}/item/',
        )

    def list_function_items(self, *, model_name: str) -> list[object]:
        return self._call_raw(
            method='GET',
            path=f'/model/{model_name}/function/',
        )

    def create_function(self, *, name: str, data: object):
        self._call_raw(
            method='POST',
            path=f'/function/{name}/',
            data=data,
        )
