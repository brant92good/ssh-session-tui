"""Grouping, searching and atomic bulk edits for the machine catalog."""
from dataclasses import replace
import shlex

from .catalog import CatalogError, group_path, parse_tags


def within(group, parent):
    return group.casefold() == parent.casefold() or bool(parent) and group.casefold().startswith(parent.casefold() + '/')


def groups(machines):
    counts = {}
    canonical = {}
    for machine in machines:
        parts = machine.group.split('/') if machine.group else []
        for depth in range(1, len(parts) + 1):
            path = '/'.join(parts[:depth])
            key = canonical.setdefault(path.casefold(), path)
            counts[key] = counts.get(key, 0) + 1
    return dict(sorted(counts.items(), key=lambda item: item[0].casefold()))


def matches(machine, query):
    haystack = ' '.join([machine.name, machine.user, machine.group, *machine.tags,
                        *(r.host for r in machine.routes), *(r.name for r in machine.routes),
                        *(r.ssh_alias or '' for r in machine.routes)]).casefold()
    try:
        terms = shlex.split(query)
    except ValueError:
        terms = query.split()
    for term in terms:
        if term.casefold().startswith('tag:'):
            if term[4:].casefold() not in {tag.casefold() for tag in machine.tags}:
                return False
        elif term.casefold().startswith('group:'):
            if not within(machine.group, term[6:]):
                return False
        elif term.casefold() not in haystack:
            return False
    return True


def filtered(machines, query='', group=None, tag=None):
    return [m for m in machines if (group is None or within(m.group, group)) and matches(m, query)
            and (tag is None or tag.casefold() in {t.casefold() for t in m.tags})]


def edit_many(catalog, machine_ids, expected, *, group=None, add_tags=(), remove_tags=()):
    snapshot = catalog.load()
    ids = set(machine_ids)
    if snapshot.revision != expected:
        raise CatalogError('The catalog changed. Reload before applying this edit.')
    if not ids or ids - {m.id for m in snapshot.machines}:
        raise CatalogError('Select machines from the current catalog.')
    if group is not None:
        group = group_path(group)
        group = next((name for name in groups(snapshot.machines) if name.casefold() == group.casefold()), group)
    added = parse_tags(add_tags)
    removed = {tag.casefold() for tag in parse_tags(remove_tags)}
    if removed & {tag.casefold() for tag in added}:
        raise CatalogError('A tag cannot be added and removed in the same edit.')
    changed = tuple(replace(m, group=m.group if group is None else group,
                            tags=parse_tags([t for t in m.tags if t.casefold() not in removed] + list(added)))
                    if m.id in ids else m for m in snapshot.machines)
    return catalog.save(changed, expected)


def rename_group(catalog, old, new, expected):
    old, new = group_path(old), group_path(new)
    snapshot = catalog.load()
    if not old or not new or not any(within(m.group, old) for m in snapshot.machines):
        raise CatalogError('Choose an existing group and a nonempty new name.')
    if new.casefold().startswith(old.casefold() + '/'):
        raise CatalogError('A group cannot be moved inside itself.')
    existing = groups(snapshot.machines)
    if old.casefold() != new.casefold() and any(g.casefold() == new.casefold() for g in existing):
        raise CatalogError('That group already exists. Use Move to merge machines into it.')
    def renamed(path):
        suffix = '/'.join(path.split('/')[len(old.split('/')):])
        return group_path(new + ('/' + suffix if suffix else ''))
    changed = tuple(replace(m, group=renamed(m.group)) if within(m.group, old) else m
                    for m in snapshot.machines)
    return catalog.save(changed, expected)
