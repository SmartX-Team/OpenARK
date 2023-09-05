#!/usr/bin/env python3
# Copyright (c) 2023 Ho Kim (ho.kim@ulagbulag.io). All rights reserved.
# Use of this source code is governed by a GPL-3-style license that can be
# found in the LICENSE file.

import subprocess
import time


def _run_shell(command: str) -> str:
    return subprocess.run(
        args=command,
        shell=True,
        capture_output=True,
    ).stdout.decode('utf-8')


class Connection:
    def __init__(self, info: str) -> None:
        parsed = [
            item
            for item in ('*' + info).split(' ')
            if item
        ]
        self._in_use = parsed[0] == '**'
        self._bssid = parsed[1]
        self._ssid = parsed[2]

        self._quality = None
        self._rate = None
        for item in parsed[::-1]:
            if item.isnumeric():
                if self._quality is None:
                    self._quality = int(item)
                else:
                    self._rate = int(item)
                    break
        self._latency: float | None = None

    def _calculate_latency(self) -> None:
        PING_SECS = 4

        parsed = _run_shell(
            'ip route '
            '| head -n 1 '
            '| awk \'{print $3}\' '
            f'| xargs ping -U -c {PING_SECS} -w {PING_SECS + 1} '
            '| tail -n 1 '
            '| awk \'{print $4}\'',
        ).strip().split('/')
        if len(parsed) == 4:
            latency_avg = float(parsed[1])
            latency_dev = float(parsed[3])
            latency = latency_avg + latency_dev
            # latency_min = float(parsed[0])
            # latency = latency_min
        else:
            latency = float(2 * PING_SECS * 1000)

        if self._latency is None:
            self._latency = latency
        else:
            self._latency = (self._latency + latency) / 2

    @classmethod
    def get_list(cls) -> list['Connection']:
        return [
            cls(info)
            for info in _run_shell(
                'nmcli device wifi list --rescan yes',
            ).split('\n')[1:]
            if info
        ]

    def calculate_score(self, base: 'Connection') -> tuple[bool, int]:
        QUALITY_PANELTY = 10

        if self._latency is not None and self._latency > base._latency:
            return (False, 0)
        if self._quality + QUALITY_PANELTY <= base._quality:
            return (False, 0)
        if self._rate < base._rate:
            return (False, 0)
        return True, self._quality - base._quality

    def connect(self, nm_connection: str) -> None:
        _run_shell(
            # f'nmcli connection modify --temporary {nm_connection} 802-11-wireless.bssid "{self._bssid}"'
            f'nmcli connection modify {nm_connection} 802-11-wireless.bssid "{self._bssid}" '
            '&& systemctl restart NetworkManager '
            '&& sleep 10'
        )

    def __repr__(self) -> str:
        return f'{"* " if self._in_use else ""}' \
            f'{self._bssid} {self._ssid} {self._rate} Mbit/s {self._quality}' \
            f'{f" {self._latency}ms" if self._latency is not None else ""}'


class ConnectionDatabase:
    def __init__(self) -> None:
        self._bssids: dict[str, Connection] = {}
        self._ssid: str | None = None

    @property
    def _nm_connection(self) -> str:
        return '10-kiss-enable-master'

    def _fetch_connection(self, info: Connection) -> Connection:
        if not self._ssid:
            self._ssid = info._ssid

        if info._bssid in self._bssids:
            fetched = self._bssids[info._bssid]
            fetched._quality = info._quality
        else:
            fetched = self._bssids[info._bssid] = info

        fetched._in_use = info._in_use
        if fetched._in_use:
            fetched._calculate_latency()
        return fetched

    def is_available(self) -> bool:
        return _run_shell(
            f'nmcli connection show {self._nm_connection} '
            '| grep -Po \'^connection\.type\: *802\-11\-wireless$\''
        ).strip() != ''

    def reset(self) -> None:
        return _run_shell(
            f'nmcli connection modify {self._nm_connection} -802-11-wireless.bssid "" '
            '&& systemctl restart NetworkManager '
            '&& sleep 10'
        )

    def update_connection(self, is_updated: bool) -> bool:
        def _candidate_score_sort_key(item: tuple[Connection, int]) -> (int, str):
            connection, score = item
            return -score, connection._bssid

        infos = Connection.get_list()
        current = self._fetch_connection(next(
            info
            for info in infos
            if info._in_use and (
                not self._ssid
                or self._ssid == info._ssid
            )
        ))

        candidates = [
            self._fetch_connection(info)
            for info in infos
            if not info._in_use and self._ssid == info._ssid
        ]
        candidates_score = [
            (candidate, *candidate.calculate_score(base=current))
            for candidate in candidates
        ]
        candidates_score_sorted: list[tuple[Connection, int]] = sorted(
            [
                (candidate, score)
                for (candidate, is_available, score) in candidates_score
                if is_available
            ],
            key=_candidate_score_sort_key,
        )
        if candidates_score_sorted:
            best, _ = candidates_score_sorted[0]
            print(f'Current: {current}')
            print(f'Switching to: {best}', flush=True)
            best.connect(self._nm_connection)
            return True
        elif is_updated:
            print(f'Current: {current}')
        return False


if __name__ == '__main__':
    connections = ConnectionDatabase()
    if not connections.is_available():
        print('Cannot find Wireless Interface')
        exit(0)
    connections.reset()

    is_updated = True
    while True:
        is_updated = connections.update_connection(is_updated)
        if is_updated:
            time.sleep(5)
        else:
            time.sleep(5 * 60)  # 5 minutes
