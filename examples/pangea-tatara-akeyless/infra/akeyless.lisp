;; Akeyless target pointing at the AWS account — a canonical
;; pangea-akeyless resource, Lisp-native.

(defteia
  :tipo      akeyless/target-aws
  :nome      primary
  :atributos (:name                "aws-primary"
              :use-gw-cloud-identity #t
              :region              "us-east-1"
              :tags                (:owner "pleme-io"
                                    :managed-by "caixa")))

(defteia
  :tipo      akeyless/dynamic-secret-aws
  :nome      readonly-role
  :atributos (:name        "aws-readonly-role"
              :target-name (ref akeyless/target-aws primary name)
              :user-ttl    "900"
              :tags        (:owner "pleme-io"
                            :access-tier "read-only")))
