#!/usr/bin/env python3

import argparse
import sys
import time

import paramiko

if __name__ == '__main__':
    # define a parser
    parser = argparse.ArgumentParser()
    parser.add_argument(
        '--ssh-hostname', type=str, required=True,
        help='SSH hostname',
    )
    parser.add_argument(
        '--ssh-username', type=str, required=True,
        help='SSH ssername',
    )
    parser.add_argument(
        '--ssh-password', type=str, required=True,
        help='SSH password',
    )
    # parser.add_argument(
    #     '--ssh-options', type=str, required=False,
    #     help='SSH options',
    # )
    parser.add_argument(
        '--command', type=str, required=True,
        help='SSH options',
    )
    parser.add_argument(
        '--buffer-size', type=int, default=1024,
        help='SSH channel output buffer size',
    )
    parser.add_argument(
        '--buffer-tick', type=float, default=0.1,
        help='SSH channel output buffer checking interval',
    )
    
    # parse command-line arguments
    args = parser.parse_args()
    
    # manipulate the command string
    commands = args.command.replace(";", "\n").split('\n')

    # open a SSH connection
    connection = paramiko.SSHClient()
    connection.set_missing_host_key_policy(paramiko.AutoAddPolicy())  # ignore host key validation
    connection.connect(
        hostname=args.ssh_hostname,
        username=args.ssh_username,
        password=args.ssh_password,
    )

    # open a SSH channel
    channel = connection.invoke_shell()

    # run a command in the channel
    stdout, stderr, retcode = '', '', None
    prompts = ['>', '> ', '$ ', '#', '# ']
    for command in commands:
        channel.send(f'{command.strip()}\n')  # append a new-line to execute

        # read the responses
        is_finished, retcode = False, None
        while not is_finished:
            time.sleep(args.buffer_tick)  # pass some time
            while channel.recv_ready():
                recv = channel.recv(args.buffer_size).decode()
                stdout += recv
                
                # try to detect prompt
                is_finished |= any(recv.strip().endswith(prompt) for prompt in prompts)
            while channel.recv_stderr_ready():
                recv = channel.recv_stderr(args.buffer_size).decode()
                stderr += recv
            if channel.exit_status_ready():
                is_finished = True
                retcode = channel.recv_exit_status()

        # exit if the connection is closed
        if retcode is not None:
            break

    # write outputs
    print(stdout, file=sys.stdout)
    print(stderr, file=sys.stderr)
    exit(retcode or 0)
