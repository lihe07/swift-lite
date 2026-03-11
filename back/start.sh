#!/bin/bash

cd /home/ecs-user/swift-lite/back

uv run sanic --fast main:app -p 20000
