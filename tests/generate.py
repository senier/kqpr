#!/usr/bin/env python3

# pylint: disable=missing-docstring

import random
import string
import sys

from pykeepass import PyKeePass, create_database
from pykeepass.group import Group


def randomname(length: int):
    length = random.randint(2, length + 1)
    return "".join(random.choice(string.ascii_lowercase) for i in range(length))


def add_random_entries(
    database: PyKeePass, max_entries: int, descent_prob: int, group: Group, depth: int = 0
):
    if depth > 5:
        return
    for _ in range(1, random.randint(1, max_entries + 1)):
        database.add_entry(
            destination_group=group,
            title=randomname(10),
            username=randomname(10),
            password=randomname(100),
        )
    if random.randint(1, 101) < descent_prob:
        for _ in range(1, random.randint(1, max_entries + 1)):
            new_group = database.add_group(destination_group=group, group_name=randomname(10))
            add_random_entries(
                database=database,
                max_entries=max_entries,
                descent_prob=descent_prob // 2,
                group=new_group,
                depth=depth + 1,
            )


if len(sys.argv) != 3:
    print(f"{sys.argv[0]} <dbpath> <password>")
    sys.exit(1)

filename = sys.argv[1]
password = sys.argv[2]

kpdb = create_database(filename=filename, password=password)
add_random_entries(database=kpdb, max_entries=10, descent_prob=99, group=kpdb.root_group)
kpdb.save()
