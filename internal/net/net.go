// +build !js !wasm

package net

import (
	"errors"
	"io"
	"net"
	"net/http"
	"os"
	"syscall"
	"time"
)

func LookupIP(host string) (addrs []net.IP, err error) {
	return net.LookupIP(host)
}

func LookupPort(network, service string) (port int, err error) {
	return net.LookupPort(network, service)
}

func Dial(network, addr string) (c net.Conn, err error) {
	c, err = net.Dial(network, addr)
	if err != nil {
		return c, err
	}
	if network == "tcp" {
		return &TCPConn{tc: c.(*net.TCPConn)}, err
	}
	panic("network not supported")
}

func ListenAndServe(addr string, handler http.Handler) error {
	return http.ListenAndServe(addr, handler)
}

// TCPConn ...
type TCPConn struct {
	tc *net.TCPConn
}

func (c *TCPConn) Read(b []byte) (ln int, err error) {
	return c.tc.Read(b)
}
func (c *TCPConn) Write(b []byte) (ln int, err error) {
	return c.tc.Write(b)
}
func (c *TCPConn) Close() error {
	return c.tc.Close()
}
func (c *TCPConn) LocalAddr() net.Addr {
	return c.tc.LocalAddr()
}
func (c *TCPConn) RemoteAddr() net.Addr {
	return c.tc.RemoteAddr()
}
func (c *TCPConn) SetDeadline(t time.Time) error {
	return c.tc.SetDeadline(t)
}
func (c *TCPConn) SetReadDeadline(t time.Time) error {
	return c.tc.SetReadDeadline(t)
}
func (c *TCPConn) SetWriteDeadline(t time.Time) error {
	return c.tc.SetWriteDeadline(t)
}
func (c *TCPConn) CloseRead() error {
	return c.tc.CloseRead()
}
func (c *TCPConn) CloseWrite() error {
	return c.tc.CloseWrite()
}
func (c *TCPConn) File() (f *os.File, err error) {
	return c.tc.File()
}
func (c *TCPConn) ReadFrom(r io.Reader) (int64, error) {
	return c.tc.ReadFrom(r)
}
func (c *TCPConn) SetKeepAlive(keepalive bool) error {
	return c.tc.SetKeepAlive(keepalive)
}
func (c *TCPConn) SetKeepAlivePeriod(d time.Duration) error {
	return c.tc.SetKeepAlivePeriod(d)
}
func (c *TCPConn) SetLinger(sec int) error {
	return c.tc.SetLinger(sec)
}
func (c *TCPConn) SetNoDelay(noDelay bool) error {
	return c.tc.SetNoDelay(noDelay)
}
func (c *TCPConn) SetReadBuffer(bytes int) error {
	return c.tc.SetReadBuffer(bytes)
}
func (c *TCPConn) SetWriteBuffer(bytes int) error {
	return c.tc.SetWriteBuffer(bytes)
}
func (c *TCPConn) SyscallConn() (syscall.RawConn, error) {
	return c.tc.SyscallConn()
}

// TCPListener ...
type TCPListener struct {
	tl *net.TCPListener
}

func (l *TCPListener) Close() error {
	return l.tl.Close()
}

func (l *TCPListener) Addr() net.Addr {
	return l.tl.Addr()
}

func (l *TCPListener) SetDeadline(t time.Time) error {
	return l.tl.SetDeadline(t)
}

func (l *TCPListener) Accept() (net.Conn, error) {
	tc, err := l.tl.Accept()
	if err != nil {
		return tc, err
	}
	switch tc := tc.(type) {
	case *net.TCPConn:
		return &TCPConn{tc: tc}, err
	}
	return nil, errors.New("TCPListener accept didn't return a tcp cnn")
}

func (l *TCPListener) AcceptTCP() (*TCPConn, error) {
	tc, err := l.tl.AcceptTCP()
	if err != nil {
		return nil, err
	}
	return &TCPConn{tc: tc}, err
}

func ListenTCP(network string, laddr *net.TCPAddr) (*TCPListener, error) {
	l, err := net.ListenTCP(network, laddr)
	if err != nil {
		return nil, err
	}
	return &TCPListener{tl: l}, nil
}

func Listen(network, addr string) (net.Listener, error) {
	return net.Listen(network, addr)
}
