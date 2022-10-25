#!/usr/bin/env python3

import argparse
import json
import os

# import 3rd-party packages
try:
    import pandas as pd
except ImportError as e:
    print(f'Please install the python package: {e.name}')


def flatten_data(y):
    ''' Flatten json data (https://stackoverflow.com/a/51379007) '''
    out = {}

    def flatten(x, name='', sep='.'):
        if type(x) is dict:
            for a in x:
                flatten(x[a], f'{name}{a}{sep}')
        elif type(x) is list:
            i = 0
            for a in x:
                flatten(a, f'{name}{i}{sep}', sep=sep)
                i += 1
        else:
            out[name[:-1]] = x

    flatten(y)
    return out


def load_dataset(data_dir: str) -> pd.DataFrame:
    ''' Load dataset from given data_dir '''
    json_files = [
        flatten_data(json.load(open(f'{data_dir}/{f}')))
        for f in os.listdir(data_dir)
        if f.endswith('.json')
    ]
    return pd.DataFrame.from_dict(json_files)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument(
        '--data_dir', type=str, help='benchmark results directory',
        default='./results',
    )
    parser.add_argument(
        '--output', type=str, help='output filename',
        default='./results.csv',
    )

    # parse arguments
    args = parser.parse_args()

    # load dataset from given data_dir
    df = load_dataset(data_dir=args.data_dir)
    
    # save the data
    df.to_csv(args.output, index=False)
