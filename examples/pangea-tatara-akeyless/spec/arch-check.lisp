;; checks.lisp-style invariants that `feira lint` + `feira tofu render`
;; require to pass. The built-in caixa-arch rulebook handles
;; unique-resource-names, unresolved refs, and public-ingress + tag
;; coverage; this file declares the caixa-specific additions.

(defcheck vpc-has-owner-tag
  (every-defteia :tipo aws/vpc
    (has-kwarg :tags
      (has-kwarg :owner))))

(defcheck every-akeyless-target-has-region
  (every-defteia :tipo akeyless/target-aws
    (has-kwarg :region)))
