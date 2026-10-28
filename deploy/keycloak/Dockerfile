# syntax=docker/dockerfile:1.28.0@sha256:bb22d9815c728170f72750f4e5b0d672e06176142e1d602c7e66c050100b7e5b
FROM quay.io/keycloak/keycloak:26.8.1@sha256:a78d6b79572168cb25886b80a8b5b8d4ed4390743e3eecb810538683e209dd6b AS upstream

FROM cgr.dev/chainguard/wolfi-base:latest@sha256:360896efb0753a5701c48f860e17130c205eae5a264797aa3e7ea126ee2923a7
# The official image's Java RPM predates the October CPU/CA trust fixes.
# Keep the patched JVM visible in the package database for vulnerability scans.
RUN apk --no-cache upgrade \
    && apk add --no-cache bash openjdk-21-jre=21.0.13.0.2-r0 ca-certificates-bundle \
    && adduser -D -H -u 1000 -G root -s /sbin/nologin keycloak
ENV JAVA_HOME=/usr/lib/jvm/java-21-openjdk
# Apply the upstream CVE-2026-114820 mitigation as well as the October CPU:
# https://ops.java/emudhra/. Wolfi's shared keystore uses these alias names.
# Check the effective trust store in the acceptance harness, not only versions.
RUN for alias in emsignrootca-g1 emsignrootca-g2 emsigneccrootca-g3 aaacertificateservices; do \
        if "$JAVA_HOME/bin/keytool" -list -cacerts -storepass changeit -alias "$alias" >/dev/null 2>&1; then \
            "$JAVA_HOME/bin/keytool" -delete -cacerts -storepass changeit -alias "$alias" || exit 1; \
        fi; \
    done \
    && "$JAVA_HOME/bin/keytool" -list -cacerts -storepass changeit >/dev/null
COPY --from=upstream --chown=1000:0 /opt/keycloak/ /opt/keycloak/
ADD --checksum=sha256:b530942257fb935f8b2cfaa5f8eb5bd59c555fd8e8d01b8ce98912e077ea606c \
    --chmod=0644 \
    https://repo.maven.apache.org/maven2/com/google/guava/guava/33.7.2-jre/guava-33.7.2-jre.jar /tmp/guava.jar
# Quarkus records the classpath filename. Keep that filename while replacing
# its contents (including Maven version metadata), then re-augment the server.
RUN test -f /opt/keycloak/lib/lib/main/com.google.guava.guava-33.6.0-jre.jar \
    && cp /tmp/guava.jar /opt/keycloak/lib/lib/main/com.google.guava.guava-33.6.0-jre.jar \
    && rm /tmp/guava.jar
USER 1000:0
RUN mkdir -m 0700 -p /opt/keycloak/data/transaction-logs \
    && /opt/keycloak/bin/kc.sh build --db=postgres --health-enabled=true --metrics-enabled=true
WORKDIR /opt/keycloak
ENTRYPOINT ["/opt/keycloak/bin/kc.sh"]
CMD ["start", "--optimized", "--import-realm"]
