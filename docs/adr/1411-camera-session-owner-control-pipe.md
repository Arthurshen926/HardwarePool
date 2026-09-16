# ADR 1411: Camera session owner controlled by pipe lifetime

Status: accepted

Date: 2026-09-16

## Context

The integrated camera receiver only decodes frames. The existing virtual-camera
lab owns a receiver and a temporary camera for a fixed GUI hold. Killing that
lab from the desktop would skip its explicit nested-process and registration
cleanup. A dedicated ownership boundary is needed before UI composition.

## Decision

Add a Windows-only camera session host with no command-line arguments. It uses
the existing fixed session/current-user camera plan and requires separately
deployed COM source registration. It refuses an existing camera registration,
starts one camera, reports a fixed stdout readiness record and retains the
camera until its inherited stdin closes. Unexpected input fails the session.
Normal EOF, parent pipe loss and handled failures run Stop and Shutdown.
Media never crosses stdin/stdout. This host does not deploy COM, launch a
receiver, change drivers or alter another camera.

## Consequences

The desktop can later own the receiver and camera host independently and close
the control pipe before resorting to bounded termination. The current decode
Quick Action is unchanged until readiness, cleanup, shared-frame publication
and exact deployed-COM preflight are composed and tested together. Process
termination by the OS cannot promise explicit cleanup; the plan is session
scoped. This slice has compile evidence only, not physical camera acceptance.
