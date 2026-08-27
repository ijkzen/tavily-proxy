# 极简打包（ADR-0004）：二进制在本机交叉编译，远端只做 COPY，零编译负担。
# 静态 musl 二进制 + rustls(webpki-roots) 内嵌 CA，scratch 即可运行。
FROM scratch
COPY tavily-proxy /tavily-proxy
EXPOSE 8080
ENTRYPOINT ["/tavily-proxy"]
