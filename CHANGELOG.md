# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->

## [Unreleased] - ReleaseDate

### Added
- Initial release of Headwind Kubernetes operator
- Deployment, StatefulSet, and DaemonSet update automation
- Flux HelmRelease update support
- Web UI with authentication (none, simple, token, proxy modes)
- Observability dashboard with multi-backend support (Prometheus, VictoriaMetrics, InfluxDB)
- Semantic versioning policy engine (patch, minor, major, all, glob, force, none)
- Webhook support for registry events (Docker Hub, Harbor, GitLab, GHCR)
- Registry polling for pull-based updates
- Approval workflow with REST API
- Rollback capabilities (manual and automatic)
- Comprehensive notifications (Slack, Microsoft Teams, generic webhooks)
- Hot-reload configuration via ConfigMap
- 35+ Prometheus metrics
- kubectl plugin for easier management

### Security
- Multi-mode Web UI authentication
- Audit logging for all approval/rejection actions
- TokenReview API integration for Kubernetes token validation
- Read-only root filesystem
- Non-root user execution (UID 1001)
- Comprehensive RBAC permissions

### Documentation
- Complete README with examples
- Docusaurus documentation site
- Web UI guides (overview, authentication, observability)
- Configuration reference
- API documentation
- Architecture documentation (CLAUDE.md)

<!-- next-url -->
[Unreleased]: https://github.com/headwind-sh/headwind/compare...HEAD
