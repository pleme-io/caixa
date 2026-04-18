;; VPC + internet gateway + subnet trio — the "secure-vpc" Pangea pattern
;; re-expressed in tatara-lisp. Every value is a TeiaInstance; (ref …)
;; forms become Terraform interpolations at emit time.

(defteia
  :tipo      aws/vpc
  :nome      main
  :atributos (:cidr-block           "10.0.0.0/16"
              :enable-dns-hostnames #t
              :enable-dns-support   #t
              :tags                 (:name    "pangea-tatara-akeyless"
                                     :owner   "pleme-io"
                                     :managed-by "caixa")))

(defteia
  :tipo      aws/internet-gateway
  :nome      main
  :atributos (:vpc-id (ref aws/vpc main id)
              :tags   (:name "pangea-tatara-akeyless-igw"
                       :owner "pleme-io")))

(defteia
  :tipo      aws/subnet
  :nome      public-1a
  :atributos (:vpc-id            (ref aws/vpc main id)
              :cidr-block        "10.0.1.0/24"
              :availability-zone "us-east-1a"
              :map-public-ip-on-launch #t
              :tags              (:name "public-1a"
                                  :tier "public"
                                  :owner "pleme-io")))

(defteia
  :tipo      aws/route-table
  :nome      public
  :atributos (:vpc-id (ref aws/vpc main id)
              :tags   (:name "public" :owner "pleme-io")))
