terraform {
  required_version = ">= 1.5"
  required_providers {
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "~> 4.52"
    }
  }
}

provider "cloudflare" {
  api_token = var.cloudflare_api_token
}

variable "cloudflare_account_id" {
  type = string
}

variable "cloudflare_api_token" {
  type      = string
  sensitive = true
}

locals {
  buckets = {
    runtime = "skippr-react-runtime"
    vectors = "skippr-react-vectors"
  }
}

resource "cloudflare_r2_bucket" "runtime" {
  account_id = var.cloudflare_account_id
  name       = local.buckets.runtime
}

resource "cloudflare_r2_bucket" "vectors" {
  account_id = var.cloudflare_account_id
  name       = local.buckets.vectors
}

output "runtime_bucket" {
  value = cloudflare_r2_bucket.runtime.name
}

output "vectors_bucket" {
  value = cloudflare_r2_bucket.vectors.name
}
