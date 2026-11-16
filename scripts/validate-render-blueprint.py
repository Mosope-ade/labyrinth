#!/usr/bin/env python3
"""Validate a reviewed staging proposal without creating provider resources."""
import hashlib
import json
import pathlib
import sys
import urllib.request

import jsonschema
import yaml


class BlueprintError(Exception):
    pass


class UniqueSafeLoader(yaml.SafeLoader):
    def construct_mapping(self, node, deep=False):
        keys = set()
        for key_node, _ in node.value:
            key = self.construct_object(key_node, deep=deep)
            if key in keys:
                raise BlueprintError("duplicate mapping key")
            keys.add(key)
        return super().construct_mapping(node, deep=deep)


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def validate(path):
    root = pathlib.Path(__file__).resolve().parent.parent
    lock = json.loads((root / "deploy/validation/render-schema.lock.json").read_text())
    if lock["url"] != "https://render.com/schema/render.yaml.json":
        raise BlueprintError("unexpected provider schema source")
    opener = urllib.request.build_opener(NoRedirect)
    with opener.open(lock["url"], timeout=15) as response:
        schema_bytes = response.read(1024 * 1024 + 1)
    if len(schema_bytes) > 1024 * 1024 or hashlib.sha256(schema_bytes).hexdigest() != lock["sha256"]:
        raise BlueprintError("provider schema changed; review before updating its lock")
    schema = json.loads(schema_bytes)

    def local_refs(value):
        if isinstance(value, dict):
            if "$ref" in value and not value["$ref"].startswith("#"):
                raise BlueprintError("external schema reference is not permitted")
            for child in value.values():
                local_refs(child)
        elif isinstance(value, list):
            for child in value:
                local_refs(child)

    local_refs(schema)
    blueprint = yaml.load(pathlib.Path(path).read_text(), Loader=UniqueSafeLoader)
    jsonschema.Draft202012Validator.check_schema(schema)
    errors = list(jsonschema.Draft202012Validator(schema).iter_errors(blueprint))
    if errors:
        # Do not echo instance values, which may contain operator configuration.
        paths = ["/" + "/".join(map(str, error.path)) for error in errors[:10]]
        raise BlueprintError("provider schema rejected fields at " + ", ".join(paths))
    for service in blueprint.get("services", []):
        if service.get("autoDeployTrigger") != "off":
            raise BlueprintError("staging proposal must disable automatic deployment")
        seen_env = set()
        for env in service.get("envVars", []):
            if env.get("key") in seen_env:
                raise BlueprintError("duplicate environment key")
            seen_env.add(env.get("key"))
            if env.get("key", "").endswith(("DATABASE_URL", "TOKEN")) and (
                env.get("sync") is not False or "value" in env
            ):
                raise BlueprintError("credentials must be operator supplied")
    print("Staging blueprint matches the pinned provider schema; no resources created")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: validate-render-blueprint.py BLUEPRINT_YAML")
    try:
        validate(sys.argv[1])
    except BlueprintError as error:
        raise SystemExit("Blueprint validation failed: " + str(error)) from None
    except Exception as error:
        raise SystemExit("Blueprint validation failed: " + type(error).__name__) from None
