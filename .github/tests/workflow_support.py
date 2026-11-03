"""Read the real release workflow without YAML 1.1 treating the `on` key as a boolean."""
from pathlib import Path
import re
import yaml

ROOT = Path(__file__).resolve().parents[2]


def workflow():
    return yaml.load((ROOT / '.github/workflows/release-on-version-bump.yaml').read_text(), Loader=yaml.BaseLoader)


def step(job, name):
    matches = [s for s in job['steps'] if s.get('name') == name]
    if len(matches) != 1:
        raise AssertionError(f'Expected one {name!r} step; found {len(matches)}')
    return matches[0]


def render(script, values):
    def replace(match):
        key = match.group(1).strip()
        if key not in values:
            raise AssertionError(f'Unresolved workflow expression: {key}')
        return values[key]
    return re.sub(r'\$\{\{(.*?)\}\}', replace, script)
