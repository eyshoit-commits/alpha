#!/usr/bin/env python3
"""Generate OpenAPI schema for the cave-daemon REST API."""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any, Dict


def sandbox_response_example() -> Dict[str, Any]:
    return {
        "id": "9f9c9872-2d9c-4c25-9c36-4e45be927834",
        "namespace": "demo",
        "name": "runner",
        "runtime": "process",
        "status": "created",
        "limits": {
            "cpu_millis": 750,
            "memory_mib": 1024,
            "disk_mib": 1024,
            "timeout_seconds": 120,
        },
        "created_at": "2025-10-18T12:34:56Z",
        "updated_at": "2025-10-18T12:34:56Z",
        "last_started_at": None,
        "last_stopped_at": None,
    }


def exec_response_example() -> Dict[str, Any]:
    return {
        "exit_code": 0,
        "stdout": "hello\n",
        "stderr": "",
        "duration_ms": 42,
        "timed_out": False,
    }


def execution_record_example() -> Dict[str, Any]:
    return {
        "command": "python",
        "args": ["-c", "print('hello')"],
        "executed_at": "2025-10-18T12:35:10Z",
        "exit_code": 0,
        "stdout": "hello\n",
        "stderr": "",
        "duration_ms": 55,
        "timed_out": False,
    }


def issued_key_example() -> Dict[str, Any]:
    return {
        "token": "bkg_demo_abcdefghijklmno",
        "info": {
            "id": "4b2a4d3a-4cbe-4b05-87a3-9528cdf6a1ed",
            "scope": {"type": "namespace", "namespace": "demo"},
            "rate_limit": 100,
            "created_at": "2025-10-18T12:00:00Z",
            "last_used_at": None,
            "expires_at": "2025-11-17T12:00:00Z",
            "key_prefix": "bkg_demo_abcd",
        },
    }


def rotation_webhook_payload_example() -> Dict[str, Any]:
    return {
        "event": "cave.auth.key.rotated",
        "key_id": "a7d6b321-2c52-4e76-9af2-2f893d4856fc",
        "previous_key_id": "4b2a4d3a-4cbe-4b05-87a3-9528cdf6a1ed",
        "rotated_at": "2025-10-18T12:10:00Z",
        "scope": {"type": "admin"},
        "owner": "admin",
        "key_prefix": "bkg_admin_new",
    }


def rotated_key_example() -> Dict[str, Any]:
    previous = issued_key_example()["info"].copy()
    current = {
        "id": "a7d6b321-2c52-4e76-9af2-2f893d4856fc",
        "scope": {"type": "admin"},
        "rate_limit": 200,
        "created_at": "2025-10-18T12:10:00Z",
        "key_prefix": "bkg_admin_new",
        "rotated_from": "4b2a4d3a-4cbe-4b05-87a3-9528cdf6a1ed",
        "rotated_at": "2025-10-18T12:10:00Z",
    }
    return {
        "token": "bkg_admin_newtokenvalue",
        "info": current,
        "previous": previous,
        "webhook": {
            "event_id": "5b0c33d4-a1d8-4a1c-9844-3d955b1b4c6e",
            "signature": "sha256=abc123...",
            "payload": rotation_webhook_payload_example(),
        },
    }


def error_example(message: str) -> Dict[str, Any]:
    return {"error": message}


def build_spec() -> Dict[str, Any]:
    components = {
        "securitySchemes": {
            "bearerAuth": {
                "type": "http",
                "scheme": "bearer",
                "bearerFormat": "API Token",
            }
        },
        "schemas": {
            "ErrorResponse": {
                "type": "object",
                "required": ["error"],
                "properties": {
                    "error": {"type": "string", "description": "Human readable error message."}
                },
                "example": error_example("sandbox 123 not found"),
            },
            "SandboxLimits": {
                "type": "object",
                "required": ["cpu_millis", "memory_mib", "disk_mib", "timeout_seconds"],
                "properties": {
                    "cpu_millis": {"type": "integer", "format": "int32", "description": "CPU time slice in milli-CPUs."},
                    "memory_mib": {"type": "integer", "format": "int64", "description": "Memory limit in MiB."},
                    "disk_mib": {"type": "integer", "format": "int64", "description": "Ephemeral disk limit in MiB."},
                    "timeout_seconds": {"type": "integer", "format": "int32", "description": "Execution timeout in seconds."},
                },
            },
            "SandboxResponse": {
                "type": "object",
                "required": [
                    "id",
                    "namespace",
                    "name",
                    "runtime",
                    "status",
                    "limits",
                    "created_at",
                    "updated_at",
                ],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "namespace": {"type": "string"},
                    "name": {"type": "string"},
                    "runtime": {"type": "string"},
                    "status": {"type": "string", "description": "Current sandbox lifecycle state."},
                    "limits": {"$ref": "#/components/schemas/SandboxLimits"},
                    "created_at": {"type": "string", "format": "date-time"},
                    "updated_at": {"type": "string", "format": "date-time"},
                    "last_started_at": {"type": "string", "format": "date-time", "nullable": True},
                    "last_stopped_at": {"type": "string", "format": "date-time", "nullable": True},
                },
                "example": sandbox_response_example(),
            },
            "CreateSandboxLimits": {
                "type": "object",
                "properties": {
                    "cpu_millis": {"type": "integer", "format": "int32"},
                    "memory_mib": {"type": "integer", "format": "int64"},
                    "disk_mib": {"type": "integer", "format": "int64"},
                    "timeout_seconds": {"type": "integer", "format": "int32"},
                },
                "description": "Optional overrides for namespace defaults.",
            },
            "CreateSandboxRequest": {
                "type": "object",
                "required": ["namespace", "name"],
                "properties": {
                    "namespace": {"type": "string"},
                    "name": {"type": "string"},
                    "runtime": {"type": "string", "description": "Sandbox runtime identifier (defaults to process)."},
                    "limits": {"$ref": "#/components/schemas/CreateSandboxLimits"},
                },
                "example": {
                    "namespace": "demo",
                    "name": "runner",
                    "runtime": "process",
                    "limits": {
                        "cpu_millis": 750,
                        "memory_mib": 1024,
                        "disk_mib": 1024,
                        "timeout_seconds": 120,
                    },
                },
            },
            "ExecRequest": {
                "type": "object",
                "required": ["command"],
                "properties": {
                    "command": {"type": "string"},
                    "args": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Command arguments.",
                    },
                    "stdin": {"type": "string", "description": "Optional STDIN payload."},
                    "timeout_ms": {"type": "integer", "format": "int64", "description": "Optional execution timeout in milliseconds."},
                },
                "example": {
                    "command": "python",
                    "args": ["-c", "print('hello')"],
                    "stdin": None,
                    "timeout_ms": 2000,
                },
            },
            "ExecResponse": {
                "type": "object",
                "required": ["duration_ms", "timed_out"],
                "properties": {
                    "exit_code": {"type": "integer", "format": "int32", "nullable": True},
                    "stdout": {"type": "string", "nullable": True},
                    "stderr": {"type": "string", "nullable": True},
                    "duration_ms": {"type": "integer", "format": "int64"},
                    "timed_out": {"type": "boolean"},
                },
                "example": exec_response_example(),
            },
            "ExecutionRecord": {
                "type": "object",
                "required": ["command", "args", "executed_at", "duration_ms", "timed_out"],
                "properties": {
                    "command": {"type": "string"},
                    "args": {"type": "array", "items": {"type": "string"}},
                    "executed_at": {"type": "string", "format": "date-time"},
                    "exit_code": {"type": "integer", "format": "int32", "nullable": True},
                    "stdout": {"type": "string", "nullable": True},
                    "stderr": {"type": "string", "nullable": True},
                    "duration_ms": {"type": "integer", "format": "int64"},
                    "timed_out": {"type": "boolean"},
                },
                "example": execution_record_example(),
            },
            "CreateKeyScope": {
                "oneOf": [
                    {
                        "type": "object",
                        "required": ["type"],
                        "properties": {
                            "type": {"type": "string", "enum": ["admin"]},
                        },
                    },
                    {
                        "type": "object",
                        "required": ["type", "namespace"],
                        "properties": {
                            "type": {"type": "string", "enum": ["namespace"]},
                            "namespace": {"type": "string"},
                        },
                    },
                ],
                "discriminator": {"propertyName": "type"},
            },
            "CreateKeyRequest": {
                "type": "object",
                "required": ["scope"],
                "properties": {
                    "scope": {"$ref": "#/components/schemas/CreateKeyScope"},
                    "rate_limit": {"type": "integer", "format": "int32", "description": "Requests per minute (defaults to 100)."},
                    "ttl_seconds": {"type": "integer", "format": "int64", "description": "Optional time-to-live for the key."},
                },
                "example": {
                    "scope": {"type": "namespace", "namespace": "demo"},
                    "rate_limit": 100,
                    "ttl_seconds": 2592000,
                },
            },
            "KeyScope": {
                "oneOf": [
                    {
                        "type": "object",
                        "required": ["type"],
                        "properties": {
                            "type": {"type": "string", "enum": ["admin"]},
                        },
                    },
                    {
                        "type": "object",
                        "required": ["type", "namespace"],
                        "properties": {
                            "type": {"type": "string", "enum": ["namespace"]},
                            "namespace": {"type": "string"},
                        },
                    },
                ],
                "discriminator": {"propertyName": "type"},
            },
            "KeyInfo": {
                "type": "object",
                "required": ["id", "scope", "rate_limit", "created_at", "key_prefix"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "scope": {"$ref": "#/components/schemas/KeyScope"},
                    "rate_limit": {"type": "integer", "format": "int32"},
                    "created_at": {"type": "string", "format": "date-time"},
                    "last_used_at": {"type": "string", "format": "date-time", "nullable": True},
                    "expires_at": {"type": "string", "format": "date-time", "nullable": True},
                    "key_prefix": {"type": "string", "description": "Truncated token prefix for audit displays."},
                    "rotated_from": {"type": "string", "format": "uuid", "nullable": True},
                    "rotated_at": {"type": "string", "format": "date-time", "nullable": True},
                },
            },
            "IssuedKeyResponse": {
                "type": "object",
                "required": ["token", "info"],
                "properties": {
                    "token": {"type": "string", "description": "Full bearer token. Shown once."},
                    "info": {"$ref": "#/components/schemas/KeyInfo"},
                },
                "example": issued_key_example(),
            },
            "RotateKeyRequest": {
                "type": "object",
                "required": ["key_id"],
                "properties": {
                    "key_id": {"type": "string", "format": "uuid"},
                    "rate_limit": {
                        "type": "integer",
                        "format": "int32",
                        "description": "Override rate limit for the rotated key (requests per minute).",
                    },
                    "ttl_seconds": {
                        "type": "integer",
                        "format": "int64",
                        "description": "Optional TTL for the rotated key in seconds.",
                    },
                },
                "example": {"key_id": "4b2a4d3a-4cbe-4b05-87a3-9528cdf6a1ed", "rate_limit": 200},
            },
            "RotationWebhookPayload": {
                "type": "object",
                "required": [
                    "event",
                    "key_id",
                    "previous_key_id",
                    "rotated_at",
                    "scope",
                    "owner",
                    "key_prefix",
                ],
                "properties": {
                    "event": {"type": "string"},
                    "key_id": {"type": "string", "format": "uuid"},
                    "previous_key_id": {"type": "string", "format": "uuid"},
                    "rotated_at": {"type": "string", "format": "date-time"},
                    "scope": {"$ref": "#/components/schemas/KeyScope"},
                    "owner": {"type": "string"},
                    "key_prefix": {"type": "string"},
                },
                "example": rotation_webhook_payload_example(),
            },
            "RotationWebhookResponse": {
                "type": "object",
                "required": ["event_id", "signature", "payload"],
                "properties": {
                    "event_id": {"type": "string", "format": "uuid"},
                    "signature": {"type": "string"},
                    "payload": {"$ref": "#/components/schemas/RotationWebhookPayload"},
                },
            },
            "RotatedKeyResponse": {
                "type": "object",
                "required": ["token", "info", "previous", "webhook"],
                "properties": {
                    "token": {"type": "string"},
                    "info": {"$ref": "#/components/schemas/KeyInfo"},
                    "previous": {"$ref": "#/components/schemas/KeyInfo"},
                    "webhook": {"$ref": "#/components/schemas/RotationWebhookResponse"},
                },
                "example": rotated_key_example(),
            },
        },
    }

    responses = {
        "400": {
            "description": "Bad request",
            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/ErrorResponse"}, "example": error_example("namespace query parameter is required")}},
        },
        "401": {
            "description": "Unauthorized",
            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/ErrorResponse"}, "example": error_example("missing Authorization bearer token")}},
        },
        "403": {
            "description": "Forbidden",
            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/ErrorResponse"}, "example": error_example("insufficient permissions for requested scope")}},
        },
        "404": {
            "description": "Not found",
            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/ErrorResponse"}, "example": error_example("sandbox 9f9c... not found")}},
        },
        "409": {
            "description": "Conflict",
            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/ErrorResponse"}, "example": error_example("sandbox 'runner' already exists in namespace 'demo'")}},
        },
        "500": {
            "description": "Internal server error",
            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/ErrorResponse"}, "example": error_example("unexpected internal error")}},
        },
    }

    paths = {
        "/healthz": {
            "get": {
                "tags": ["Operations"],
                "summary": "Health probe",
                "operationId": "getHealthz",
                "responses": {
                    "200": {"description": "Service is healthy"}
                },
            }
        },
        "/metrics": {
            "get": {
                "tags": ["Operations"],
                "summary": "Prometheus metrics",
                "operationId": "getMetrics",
                "responses": {
                    "200": {
                        "description": "Metrics payload",
                        "content": {
                            "text/plain": {
                                "schema": {"type": "string"},
                                "example": "# metrics\nbkg_cave_daemon_up 1\n",
                            }
                        },
                    }
                },
            }
        },
        "/api/v1/sandboxes": {
            "post": {
                "tags": ["Sandboxes"],
                "summary": "Create a sandbox",
                "operationId": "createSandbox",
                "security": [{"bearerAuth": []}],
                "requestBody": {
                    "required": True,
                    "content": {
                        "application/json": {
                            "schema": {"$ref": "#/components/schemas/CreateSandboxRequest"},
                            "example": components["schemas"]["CreateSandboxRequest"]["example"],
                        }
                    },
                },
                "responses": {
                    "200": {
                        "description": "Sandbox created",
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/SandboxResponse"},
                                "example": sandbox_response_example(),
                            }
                        },
                    },
                    "400": responses["400"],
                    "401": responses["401"],
                    "403": responses["403"],
                    "409": responses["409"],
                    "500": responses["500"],
                },
            },
            "get": {
                "tags": ["Sandboxes"],
                "summary": "List sandboxes in a namespace",
                "operationId": "listSandboxes",
                "security": [{"bearerAuth": []}],
                "parameters": [
                    {
                        "name": "namespace",
                        "in": "query",
                        "required": True,
                        "schema": {"type": "string"},
                        "description": "Namespace identifier to filter sandboxes.",
                    }
                ],
                "responses": {
                    "200": {
                        "description": "Sandboxes",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "array",
                                    "items": {"$ref": "#/components/schemas/SandboxResponse"},
                                },
                                "example": [sandbox_response_example()],
                            }
                        },
                    },
                    "400": responses["400"],
                    "401": responses["401"],
                    "403": responses["403"],
                    "500": responses["500"],
                },
            },
        },
        "/api/v1/sandboxes/{id}/status": {
            "get": {
                "tags": ["Sandboxes"],
                "summary": "Inspect sandbox",
                "operationId": "getSandbox",
                "security": [{"bearerAuth": []}],
                "parameters": [
                    {
                        "name": "id",
                        "in": "path",
                        "required": True,
                        "schema": {"type": "string", "format": "uuid"},
                    }
                ],
                "responses": {
                    "200": {
                        "description": "Sandbox details",
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/SandboxResponse"},
                                "example": sandbox_response_example(),
                            }
                        },
                    },
                    "401": responses["401"],
                    "403": responses["403"],
                    "404": responses["404"],
                    "500": responses["500"],
                },
            }
        },
        "/api/v1/sandboxes/{id}/start": {
            "post": {
                "tags": ["Sandboxes"],
                "summary": "Start a sandbox",
                "operationId": "startSandbox",
                "security": [{"bearerAuth": []}],
                "parameters": [
                    {
                        "name": "id",
                        "in": "path",
                        "required": True,
                        "schema": {"type": "string", "format": "uuid"},
                    }
                ],
                "responses": {
                    "200": {
                        "description": "Sandbox started",
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/SandboxResponse"},
                                "example": {**sandbox_response_example(), "status": "running", "last_started_at": "2025-10-18T12:35:05Z"},
                            }
                        },
                    },
                    "401": responses["401"],
                    "403": responses["403"],
                    "404": responses["404"],
                    "409": responses["409"],
                    "500": responses["500"],
                },
            }
        },
        "/api/v1/sandboxes/{id}/exec": {
            "post": {
                "tags": ["Sandboxes"],
                "summary": "Execute a command inside a sandbox",
                "operationId": "execSandbox",
                "security": [{"bearerAuth": []}],
                "parameters": [
                    {
                        "name": "id",
                        "in": "path",
                        "required": True,
                        "schema": {"type": "string", "format": "uuid"},
                    }
                ],
                "requestBody": {
                    "required": True,
                    "content": {
                        "application/json": {
                            "schema": {"$ref": "#/components/schemas/ExecRequest"},
                            "example": components["schemas"]["ExecRequest"]["example"],
                        }
                    },
                },
                "responses": {
                    "200": {
                        "description": "Execution result",
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/ExecResponse"},
                                "example": exec_response_example(),
                            }
                        },
                    },
                    "401": responses["401"],
                    "403": responses["403"],
                    "404": responses["404"],
                    "500": responses["500"],
                },
            }
        },
        "/api/v1/sandboxes/{id}/stop": {
            "post": {
                "tags": ["Sandboxes"],
                "summary": "Stop a sandbox",
                "operationId": "stopSandbox",
                "security": [{"bearerAuth": []}],
                "parameters": [
                    {
                        "name": "id",
                        "in": "path",
                        "required": True,
                        "schema": {"type": "string", "format": "uuid"},
                    }
                ],
                "responses": {
                    "204": {"description": "Sandbox stopped"},
                    "401": responses["401"],
                    "403": responses["403"],
                    "404": responses["404"],
                    "409": responses["409"],
                    "500": responses["500"],
                },
            }
        },
        "/api/v1/sandboxes/{id}": {
            "delete": {
                "tags": ["Sandboxes"],
                "summary": "Delete a sandbox",
                "operationId": "deleteSandbox",
                "security": [{"bearerAuth": []}],
                "parameters": [
                    {
                        "name": "id",
                        "in": "path",
                        "required": True,
                        "schema": {"type": "string", "format": "uuid"},
                    }
                ],
                "responses": {
                    "204": {"description": "Sandbox deleted"},
                    "401": responses["401"],
                    "403": responses["403"],
                    "404": responses["404"],
                    "500": responses["500"],
                },
            }
        },
        "/api/v1/sandboxes/{id}/executions": {
            "get": {
                "tags": ["Sandboxes"],
                "summary": "List recent executions",
                "operationId": "listSandboxExecutions",
                "security": [{"bearerAuth": []}],
                "parameters": [
                    {
                        "name": "id",
                        "in": "path",
                        "required": True,
                        "schema": {"type": "string", "format": "uuid"},
                    },
                    {
                        "name": "limit",
                        "in": "query",
                        "required": False,
                        "schema": {"type": "integer", "format": "int32", "minimum": 1, "maximum": 100},
                        "description": "Maximum number of execution records (default 20).",
                    },
                ],
                "responses": {
                    "200": {
                        "description": "Execution history",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "array",
                                    "items": {"$ref": "#/components/schemas/ExecutionRecord"},
                                },
                                "example": [execution_record_example()],
                            }
                        },
                    },
                    "401": responses["401"],
                    "403": responses["403"],
                    "404": responses["404"],
                    "500": responses["500"],
                },
            }
        },
        "/api/v1/auth/keys": {
            "post": {
                "tags": ["Auth"],
                "summary": "Issue an API key",
                "operationId": "issueKey",
                "security": [{"bearerAuth": []}, {}],
                "requestBody": {
                    "required": True,
                    "content": {
                        "application/json": {
                            "schema": {"$ref": "#/components/schemas/CreateKeyRequest"},
                            "example": components["schemas"]["CreateKeyRequest"]["example"],
                        }
                    },
                },
                "responses": {
                    "201": {
                        "description": "API key issued",
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/IssuedKeyResponse"},
                                "example": issued_key_example(),
                            }
                        },
                    },
                    "401": responses["401"],
                    "403": responses["403"],
                    "500": responses["500"],
                },
            },
            "get": {
                "tags": ["Auth"],
                "summary": "List issued API keys",
                "operationId": "listKeys",
                "security": [{"bearerAuth": []}],
                "responses": {
                    "200": {
                        "description": "Known API keys",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "array",
                                    "items": {"$ref": "#/components/schemas/KeyInfo"},
                                },
                                "example": [issued_key_example()["info"]],
                            }
                        },
                    },
                    "401": responses["401"],
                    "403": responses["403"],
                    "500": responses["500"],
                },
            },
        },
        "/api/v1/auth/keys/rotate": {
            "post": {
                "tags": ["Auth"],
                "summary": "Rotate an API key",
                "operationId": "rotateKey",
                "security": [{"bearerAuth": []}],
                "requestBody": {
                    "required": True,
                    "content": {
                        "application/json": {
                            "schema": {"$ref": "#/components/schemas/RotateKeyRequest"},
                            "example": components["schemas"]["RotateKeyRequest"]["example"],
                        }
                    },
                },
                "responses": {
                    "200": {
                        "description": "API key rotated",
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/RotatedKeyResponse"},
                                "example": rotated_key_example(),
                            }
                        },
                    },
                    "400": responses["400"],
                    "401": responses["401"],
                    "403": responses["403"],
                    "404": responses["404"],
                    "500": responses["500"],
                },
            }
        },
        "/api/v1/auth/keys/rotated": {
            "post": {
                "tags": ["Auth"],
                "summary": "Verify a rotation webhook payload",
                "operationId": "verifyRotationWebhook",
                "security": [{"bearerAuth": []}],
                "parameters": [
                    {
                        "name": "X-Cave-Webhook-Signature",
                        "in": "header",
                        "required": True,
                        "schema": {"type": "string"},
                        "description": "HMAC signature generated with CAVE_ROTATION_WEBHOOK_SECRET.",
                    }
                ],
                "requestBody": {
                    "required": True,
                    "content": {
                        "application/json": {
                            "schema": {"$ref": "#/components/schemas/RotationWebhookPayload"},
                            "example": rotation_webhook_payload_example(),
                        }
                    },
                },
                "responses": {
                    "204": {"description": "Webhook accepted"},
                    "401": responses["401"],
                    "403": responses["403"],
                    "500": responses["500"],
                },
            }
        },
        "/api/v1/auth/keys/{id}": {
            "delete": {
                "tags": ["Auth"],
                "summary": "Revoke an API key",
                "operationId": "revokeKey",
                "security": [{"bearerAuth": []}],
                "parameters": [
                    {
                        "name": "id",
                        "in": "path",
                        "required": True,
                        "schema": {"type": "string", "format": "uuid"},
                    }
                ],
                "responses": {
                    "204": {"description": "Key revoked"},
                    "401": responses["401"],
                    "403": responses["403"],
                    "404": responses["404"],
                    "500": responses["500"],
                },
            }
        },
    }

    return {
        "openapi": "3.0.3",
        "info": {
            "title": "Cave Daemon API",
            "version": "0.1.0",
            "description": (
                "REST interface for managing sandboxes, executing workloads, and issuing API keys. "
                "See docs/api.md for narrative documentation."
            ),
        },
        "servers": [
            {"url": "http://localhost:8080", "description": "Local development"}
        ],
        "tags": [
            {"name": "Operations", "description": "Health and telemetry endpoints."},
            {"name": "Sandboxes", "description": "Sandbox lifecycle management."},
            {"name": "Auth", "description": "API key issuance and revocation."},
        ],
        "paths": paths,
        "components": components,
    }


def main(argv: list[str]) -> None:
    output = Path(argv[1]) if len(argv) > 1 else Path("openapi.yaml")
    spec = build_spec()
    output.write_text(json.dumps(spec, indent=2) + "\n")


if __name__ == "__main__":
    main(sys.argv)
