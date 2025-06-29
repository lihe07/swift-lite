#!/bin/bash

cd /home/ecs-user/swift-lite/back

/home/ecs-user/.local/bin/sanic --fast main:app -p 20000
