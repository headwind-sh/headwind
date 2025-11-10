{{/*
Expand the name of the chart.
*/}}
{{- define "headwind.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
We truncate at 63 chars because some Kubernetes name fields are limited to this (by the DNS naming spec).
If release name contains chart name it will be used as a full name.
*/}}
{{- define "headwind.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "headwind.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "headwind.labels" -}}
helm.sh/chart: {{ include "headwind.chart" . }}
{{ include "headwind.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels
*/}}
{{- define "headwind.selectorLabels" -}}
app.kubernetes.io/name: {{ include "headwind.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Create the name of the service account to use
*/}}
{{- define "headwind.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "headwind.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Create the image name
*/}}
{{- define "headwind.image" -}}
{{- $tag := .Values.image.tag | default .Chart.AppVersion }}
{{- printf "%s:%s" .Values.image.repository $tag }}
{{- end }}

{{/*
Create the name of the config map
*/}}
{{- define "headwind.configMapName" -}}
{{- printf "%s-config" (include "headwind.fullname" .) }}
{{- end }}

{{/*
Create the name of the secret
*/}}
{{- define "headwind.secretName" -}}
{{- if .Values.secret.name }}
{{- .Values.secret.name }}
{{- else if .Values.notifications.existingSecret }}
{{- .Values.notifications.existingSecret }}
{{- else }}
{{- printf "%s-secrets" (include "headwind.fullname" .) }}
{{- end }}
{{- end }}

{{/*
Return the appropriate apiVersion for RBAC
*/}}
{{- define "headwind.rbac.apiVersion" -}}
{{- if .Capabilities.APIVersions.Has "rbac.authorization.k8s.io/v1" }}
{{- print "rbac.authorization.k8s.io/v1" }}
{{- else }}
{{- print "rbac.authorization.k8s.io/v1beta1" }}
{{- end }}
{{- end }}

{{/*
Return the appropriate apiVersion for networking
*/}}
{{- define "headwind.ingress.apiVersion" -}}
{{- if .Capabilities.APIVersions.Has "networking.k8s.io/v1" }}
{{- print "networking.k8s.io/v1" }}
{{- else if .Capabilities.APIVersions.Has "networking.k8s.io/v1beta1" }}
{{- print "networking.k8s.io/v1beta1" }}
{{- else }}
{{- print "extensions/v1beta1" }}
{{- end }}
{{- end }}

{{/*
Return the appropriate apiVersion for PodDisruptionBudget
*/}}
{{- define "headwind.pdb.apiVersion" -}}
{{- if .Capabilities.APIVersions.Has "policy/v1/PodDisruptionBudget" }}
{{- print "policy/v1" }}
{{- else }}
{{- print "policy/v1beta1" }}
{{- end }}
{{- end }}

{{/*
Return the UI URL for notifications
*/}}
{{- define "headwind.uiUrl" -}}
{{- if .Values.env.HEADWIND_UI_URL }}
{{- .Values.env.HEADWIND_UI_URL }}
{{- else if .Values.ingress.enabled }}
{{- $host := index .Values.ingress.hosts 0 }}
{{- if .Values.ingress.tls }}
{{- printf "https://%s" $host.host }}
{{- else }}
{{- printf "http://%s" $host.host }}
{{- end }}
{{- else }}
{{- printf "http://%s.%s.svc.cluster.local:%d" (include "headwind.fullname" .) .Release.Namespace (int .Values.service.uiPort) }}
{{- end }}
{{- end }}

{{/*
Return the InfluxDB secret name
*/}}
{{- define "headwind.influxdb.secretName" -}}
{{- if .Values.observability.influxdb.existingSecret }}
{{- .Values.observability.influxdb.existingSecret }}
{{- else }}
{{- printf "%s-influxdb" (include "headwind.fullname" .) }}
{{- end }}
{{- end }}

{{/*
Return the InfluxDB URL
*/}}
{{- define "headwind.influxdb.url" -}}
{{- printf "http://%s-influxdb:8086" (include "headwind.fullname" .) }}
{{- end }}
